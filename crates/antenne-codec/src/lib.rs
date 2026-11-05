//! Encodeurs audio d'Antenne.
//!
//! Entrée commune : trames PCM `f32` entrelacées (voir `antenne_core::AudioFrame`).
//! Sortie : charge utile prête à être mise en paquets RTP / HTTP.

use antenne_core::{Channels, Encoding};

pub mod g711;
#[cfg(feature = "opus")]
mod opus_enc;
pub mod pcm;

/// Erreurs d'encodage.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("codec « {codec} » non compilé : reconstruire avec `--features {feature}`")]
    NotCompiled {
        codec: &'static str,
        feature: &'static str,
    },
    #[error("paramètres non supportés : {0}")]
    Unsupported(String),
    #[error("erreur d'encodage : {0}")]
    Encode(String),
}

/// Encodeur à état, une instance par flux (ou par auditeur si besoin).
pub trait Encoder: Send {
    /// Nom court du codec (`opus`, `g711u`…).
    fn codec(&self) -> &'static str;
    /// Fréquence du flux encodé (horloge RTP), ex. 48000 pour Opus, 8000 pour G.711.
    fn output_sample_rate(&self) -> u32;
    /// Encode une trame complète. `out` est vidé puis rempli.
    fn encode(&mut self, pcm: &[f32], out: &mut Vec<u8>) -> Result<(), CodecError>;
}

/// Construit l'encodeur correspondant à la configuration d'une carte.
pub fn make_encoder(
    encoding: &Encoding,
    channels: Channels,
    sample_rate: u32,
) -> Result<Box<dyn Encoder>, CodecError> {
    match encoding {
        Encoding::Pcm16 => Ok(Box::new(pcm::Pcm16Encoder::new(sample_rate, false))),
        Encoding::G711u => Ok(Box::new(g711::G711Encoder::new(
            g711::Law::MuLaw,
            channels,
            sample_rate,
        )?)),
        Encoding::G711a => Ok(Box::new(g711::G711Encoder::new(
            g711::Law::ALaw,
            channels,
            sample_rate,
        )?)),
        #[cfg(feature = "opus")]
        Encoding::Opus { bitrate_kbps, fec } => Ok(Box::new(opus_enc::OpusEncoder::new(
            sample_rate,
            channels,
            *bitrate_kbps,
            *fec,
        )?)),
        #[cfg(not(feature = "opus"))]
        Encoding::Opus { .. } => Err(CodecError::NotCompiled {
            codec: "opus",
            feature: "antenne-codec/opus",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_g711_and_pcm() {
        assert_eq!(
            make_encoder(&Encoding::G711u, Channels::Mono, 48_000)
                .unwrap()
                .codec(),
            "g711u"
        );
        assert_eq!(
            make_encoder(&Encoding::Pcm16, Channels::Stereo, 48_000)
                .unwrap()
                .codec(),
            "pcm16"
        );
    }

    #[cfg(not(feature = "opus"))]
    #[test]
    fn opus_reports_missing_feature() {
        let err = make_encoder(&Encoding::default(), Channels::Stereo, 48_000)
            .err()
            .unwrap();
        assert!(matches!(err, CodecError::NotCompiled { .. }));
    }

    #[cfg(feature = "opus")]
    #[test]
    fn opus_encodes_a_10ms_frame() {
        let mut enc = make_encoder(&Encoding::default(), Channels::Stereo, 48_000).unwrap();
        let pcm = vec![0.0_f32; 480 * 2];
        let mut out = Vec::new();
        enc.encode(&pcm, &mut out).unwrap();
        assert!(!out.is_empty());
    }
}
