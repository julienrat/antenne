//! PCM 16 bits.

use crate::{CodecError, Encoder};

/// Convertit un échantillon `f32` (`[-1, 1]`) en `i16`, avec saturation.
#[inline]
pub fn f32_to_i16(sample: f32) -> i16 {
    // `as` sature depuis Rust 1.45 : pas de débordement possible.
    (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// PCM 16 bits entrelacé. Little-endian (WAV) ou big-endian (RTP L16).
#[derive(Debug)]
pub struct Pcm16Encoder {
    sample_rate: u32,
    big_endian: bool,
}

impl Pcm16Encoder {
    pub fn new(sample_rate: u32, big_endian: bool) -> Self {
        Self {
            sample_rate,
            big_endian,
        }
    }
}

impl Encoder for Pcm16Encoder {
    fn codec(&self) -> &'static str {
        "pcm16"
    }

    fn output_sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn encode(&mut self, pcm: &[f32], out: &mut Vec<u8>) -> Result<(), CodecError> {
        out.clear();
        out.reserve(pcm.len() * 2);
        for &s in pcm {
            let v = f32_to_i16(s);
            let bytes = if self.big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            };
            out.extend_from_slice(&bytes);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_and_saturates() {
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(1.0), 32767);
        assert_eq!(f32_to_i16(-1.0), -32767);
        assert_eq!(f32_to_i16(4.0), 32767);
        assert_eq!(f32_to_i16(f32::NAN), 0);
    }

    #[test]
    fn little_endian_output() {
        let mut enc = Pcm16Encoder::new(48_000, false);
        let mut out = Vec::new();
        enc.encode(&[1.0, -1.0], &mut out).unwrap();
        assert_eq!(out, vec![0xFF, 0x7F, 0x01, 0x80]);
    }
}
