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
