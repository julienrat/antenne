//! G.711 (ITU-T) µ-law / A-law, 8 kHz mono.
//!
//! Choisi pour les ESP32 les plus modestes : décodage quasi gratuit en CPU.
//! Le sous-échantillonnage 48 kHz -> 8 kHz est volontairement simple
//! (moyenne sur `facteur` échantillons). TODO(jalon 2) : remplacer par un vrai
//! rééchantillonneur (crate `rubato`) si la qualité est insuffisante.

use antenne_core::Channels;

use crate::pcm::f32_to_i16;
use crate::{CodecError, Encoder};

/// Fréquence fixe de G.711.
pub const G711_RATE: u32 = 8_000;

/// Variante de G.711.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Law {
    /// µ-law, PCMU (Amérique du Nord / Japon), type RTP statique 0.
    MuLaw,
    /// A-law, PCMA (Europe), type RTP statique 8.
    ALaw,
}

/// Encode un échantillon 16 bits en µ-law.
pub fn linear_to_ulaw(sample: i16) -> u8 {
    const BIAS: i32 = 0x84;
    const CLIP: i32 = 32_635;

    let mut s = i32::from(sample);
    let sign = if s < 0 {
        s = -s;
        0x80
    } else {
        0x00
    };
    s = s.min(CLIP) + BIAS;

    let mut exponent: i32 = 7;
    let mut mask: i32 = 0x4000;
    while exponent > 0 && (s & mask) == 0 {
        exponent -= 1;
        mask >>= 1;
    }
    let mantissa = (s >> (exponent + 3)) & 0x0F;
    !((sign | (exponent << 4) | mantissa) as u8)
}

/// Encode un échantillon 16 bits en A-law.
pub fn linear_to_alaw(sample: i16) -> u8 {
    let mut s = i32::from(sample);
    let sign = if s >= 0 {
        0x80
    } else {
        s = -s - 1;
        0x00
    };

    let (exponent, mantissa) = if s >= 256 {
        let mut exponent: i32 = 7;
        let mut mask: i32 = 0x4000;
        while (s & mask) == 0 {
            exponent -= 1;
            mask >>= 1;
        }
        (exponent, (s >> (exponent + 3)) & 0x0F)
    } else {
        (0, (s >> 4) & 0x0F)
    };
    ((sign | (exponent << 4) | mantissa) as u8) ^ 0x55
}

/// Encodeur G.711 avec sous-échantillonnage entier vers 8 kHz.
#[derive(Debug)]
pub struct G711Encoder {
    law: Law,
    factor: usize,
}

impl G711Encoder {
    pub fn new(law: Law, channels: Channels, input_rate: u32) -> Result<Self, CodecError> {
        if channels != Channels::Mono {
            return Err(CodecError::Unsupported("G.711 est mono uniquement".into()));
        }
        let factor = input_rate / G711_RATE;
        if factor == 0 || factor * G711_RATE != input_rate {
            return Err(CodecError::Unsupported(format!(
                "G.711 : {input_rate} Hz n'est pas un multiple de 8000 Hz"
            )));
        }
        Ok(Self {
            law,
            factor: factor as usize,
        })
    }
}

impl Encoder for G711Encoder {
    fn codec(&self) -> &'static str {
        match self.law {
            Law::MuLaw => "g711u",
            Law::ALaw => "g711a",
        }
    }

    fn output_sample_rate(&self) -> u32 {
        G711_RATE
    }

    fn encode(&mut self, pcm: &[f32], out: &mut Vec<u8>) -> Result<(), CodecError> {
        out.clear();
        out.reserve(pcm.len() / self.factor);
        for chunk in pcm.chunks_exact(self.factor) {
            let avg = chunk.iter().sum::<f32>() / self.factor as f32;
            let s = f32_to_i16(avg);
            out.push(match self.law {
                Law::MuLaw => linear_to_ulaw(s),
                Law::ALaw => linear_to_alaw(s),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulaw_reference_values() {
        assert_eq!(linear_to_ulaw(0), 0xFF);
        assert_eq!(linear_to_ulaw(-1), 0x7F);
        assert_eq!(linear_to_ulaw(i16::MAX), 0x80);
        assert_eq!(linear_to_ulaw(i16::MIN), 0x00);
    }

    #[test]
    fn alaw_reference_values() {
        assert_eq!(linear_to_alaw(0), 0xD5);
        assert_eq!(linear_to_alaw(-1), 0x55);
        assert_eq!(linear_to_alaw(i16::MAX), 0xAA);
        assert_eq!(linear_to_alaw(i16::MIN), 0x2A);
    }

    #[test]
    fn ten_ms_at_48k_gives_80_bytes() {
        let mut enc = G711Encoder::new(Law::ALaw, Channels::Mono, 48_000).unwrap();
        let mut out = Vec::new();
        enc.encode(&[0.0; 480], &mut out).unwrap();
        assert_eq!(out.len(), 80);
        assert!(out.iter().all(|&b| b == 0xD5));
    }

    #[test]
    fn rejects_stereo_and_bad_rates() {
        assert!(G711Encoder::new(Law::MuLaw, Channels::Stereo, 48_000).is_err());
        assert!(G711Encoder::new(Law::MuLaw, Channels::Mono, 44_100).is_err());
    }
}
