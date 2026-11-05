//! Trame audio échangée entre la capture, l'encodage et le transport.

use std::sync::Arc;
use std::time::Instant;

/// Bloc de PCM `f32` entrelacé, de durée fixe (`audio.frame_ms`).
///
/// Le contenu est partagé (`Arc`) : diffuser une trame vers N auditeurs ne
/// copie pas les échantillons.
#[derive(Debug, Clone)]
pub struct AudioFrame {
    /// Numéro de séquence croissant, par carte.
    pub seq: u64,
    /// Fréquence d'échantillonnage (Hz).
    pub sample_rate: u32,
    /// Nombre de canaux (1 ou 2).
    pub channels: u16,
    /// Échantillons entrelacés, dans `[-1.0, 1.0]`.
    pub samples: Arc<[f32]>,
    /// Instant où la trame a été complétée côté capture (mesure de latence).
    pub captured_at: Instant,
}

impl AudioFrame {
    /// Nombre d'échantillons par canal.
    pub fn frames_per_channel(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / usize::from(self.channels)
        }
    }

    /// Durée de la trame en microsecondes.
    pub fn duration_us(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        self.frames_per_channel() as u64 * 1_000_000 / u64::from(self.sample_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_of_10ms_stereo_frame() {
        let frame = AudioFrame {
            seq: 0,
            sample_rate: 48_000,
            channels: 2,
            samples: vec![0.0; 960].into(),
            captured_at: Instant::now(),
        };
        assert_eq!(frame.frames_per_channel(), 480);
        assert_eq!(frame.duration_us(), 10_000);
    }
}
