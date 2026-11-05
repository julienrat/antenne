//! Opus via libopus (feature `opus`).
//!
//! Réglé pour la faible latence : `Application::LowDelay` (mode CELT seul),
//! trames de 10 ms par défaut (`audio.frame_ms`).

use antenne_core::Channels;

use crate::{CodecError, Encoder};

/// Taille max recommandée d'un paquet Opus.
const MAX_PACKET: usize = 4000;

pub struct OpusEncoder {
    inner: ::opus::Encoder,
    sample_rate: u32,
}

impl std::fmt::Debug for OpusEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpusEncoder")
            .field("sample_rate", &self.sample_rate)
            .finish_non_exhaustive()
    }
}

fn err(e: ::opus::Error) -> CodecError {
    CodecError::Encode(e.to_string())
}

impl OpusEncoder {
    pub fn new(
        sample_rate: u32,
        channels: Channels,
        bitrate_kbps: u32,
        fec: bool,
    ) -> Result<Self, CodecError> {
        let ch = match channels {
            Channels::Mono => ::opus::Channels::Mono,
            Channels::Stereo => ::opus::Channels::Stereo,
        };
        let mut inner =
            ::opus::Encoder::new(sample_rate, ch, ::opus::Application::LowDelay).map_err(err)?;
        let bits = i32::try_from(bitrate_kbps * 1000)
            .map_err(|_| CodecError::Unsupported("bitrate trop élevé".into()))?;
        inner
            .set_bitrate(::opus::Bitrate::Bits(bits))
            .map_err(err)?;
        inner.set_inband_fec(fec).map_err(err)?;
        Ok(Self { inner, sample_rate })
    }
}

impl Encoder for OpusEncoder {
    fn codec(&self) -> &'static str {
        "opus"
    }

    fn output_sample_rate(&self) -> u32 {
        // L'horloge RTP d'Opus est toujours 48 kHz (RFC 7587).
        48_000
    }

    fn encode(&mut self, pcm: &[f32], out: &mut Vec<u8>) -> Result<(), CodecError> {
        out.clear();
        out.resize(MAX_PACKET, 0);
        let len = self.inner.encode_float(pcm, out).map_err(err)?;
        out.truncate(len);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode 1 s de sinusoïde (amplitude 0,2) par trames de 10 ms, décode, et
    /// vérifie la crête sur la seconde moitié (après la mise en route).
    fn round_trip_peak(channels: Channels) -> f32 {
        let n = usize::from(channels.count());
        let mut enc = OpusEncoder::new(48_000, channels, 96, false).unwrap();
        let ch = if n == 1 {
            ::opus::Channels::Mono
        } else {
            ::opus::Channels::Stereo
        };
        let mut dec = ::opus::Decoder::new(48_000, ch).unwrap();
        let mut packet = Vec::new();
        let mut decoded = vec![0.0f32; 480 * n];
        let mut peak = 0.0f32;
        for f in 0..100 {
            let pcm: Vec<f32> = (0..480)
                .flat_map(|i| {
                    let t = (f * 480 + i) as f32 / 48_000.0;
                    let s = 0.2 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
                    std::iter::repeat_n(s, n)
                })
                .collect();
            enc.encode(&pcm, &mut packet).unwrap();
            let got = dec.decode_float(&packet, &mut decoded, false).unwrap();
            assert_eq!(got, 480);
            if f >= 50 {
                peak = decoded.iter().fold(peak, |p, s| p.max(s.abs()));
            }
        }
        peak
    }

    #[test]
    fn mono_round_trip_keeps_level() {
        let peak = round_trip_peak(Channels::Mono);
        assert!((0.17..0.23).contains(&peak), "crête {peak}");
    }

    #[test]
    fn stereo_round_trip_keeps_level() {
        let peak = round_trip_peak(Channels::Stereo);
        assert!((0.17..0.23).contains(&peak), "crête {peak}");
    }
}
