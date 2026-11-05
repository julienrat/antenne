//! Vu-mètres : niveau crête par canal, pour l'interface web.
//!
//! Une tâche par carte s'abonne au `broadcast` des trames et tient à jour une
//! crête « à retombée » (comme un vu-mètre matériel). Hors temps réel : on est
//! dans tokio, pas dans le callback audio.

use std::sync::atomic::{AtomicU32, Ordering};

use antenne_core::AudioFrame;
use tokio::sync::broadcast;

/// Facteur de retombée appliqué à chaque trame (≈ 20 dB/s avec des trames de 10 ms).
const DECAY_PER_FRAME: f32 = 0.955;

/// Niveaux crête courants (linéaires, `0.0..=1.0`), au plus deux canaux.
#[derive(Debug, Default)]
pub struct Meter {
    peaks: [AtomicU32; 2],
}

impl Meter {
    /// Niveaux des `channels` premiers canaux.
    pub fn levels(&self, channels: usize) -> Vec<f32> {
        self.peaks
            .iter()
            .take(channels)
            .map(|p| f32::from_bits(p.load(Ordering::Relaxed)))
            .collect()
    }

    /// Intègre une trame : crête de la trame, ou retombée de la valeur précédente.
    pub fn update(&self, frame: &AudioFrame) {
        let channels = usize::from(frame.channels).clamp(1, 2);
        let mut frame_peaks = [0.0f32; 2];
        for chunk in frame
            .samples
            .chunks_exact(usize::from(frame.channels.max(1)))
        {
            for (peak, sample) in frame_peaks.iter_mut().zip(chunk.iter().take(channels)) {
                *peak = peak.max(sample.abs());
            }
        }
        for (slot, fresh) in self.peaks.iter().zip(frame_peaks).take(channels) {
            let previous = f32::from_bits(slot.load(Ordering::Relaxed));
            let level = fresh.max(previous * DECAY_PER_FRAME).min(1.0);
            slot.store(level.to_bits(), Ordering::Relaxed);
        }
    }

    fn reset(&self) {
        for slot in &self.peaks {
            slot.store(0.0f32.to_bits(), Ordering::Relaxed);
        }
    }
}

/// Alimente `meter` tant que la carte produit des trames.
pub async fn run(meter: std::sync::Arc<Meter>, mut frames: broadcast::Receiver<AudioFrame>) {
    loop {
        match frames.recv().await {
            Ok(frame) => meter.update(&frame),
            // En retard : on saute les trames manquées, le vu-mètre n'en souffre pas.
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => {
                meter.reset();
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use super::*;

    fn frame(channels: u16, samples: Vec<f32>) -> AudioFrame {
        AudioFrame {
            seq: 0,
            sample_rate: 48_000,
            channels,
            samples: Arc::from(samples),
            captured_at: Instant::now(),
        }
    }

    #[test]
    fn stereo_peaks_are_per_channel_and_decay() {
        let meter = Meter::default();
        meter.update(&frame(2, vec![0.5, -0.1, -0.8, 0.2]));
        assert_eq!(meter.levels(2), vec![0.8, 0.2]);

        meter.update(&frame(2, vec![0.0; 4]));
        let decayed = meter.levels(2);
        assert!((decayed[0] - 0.8 * DECAY_PER_FRAME).abs() < 1e-6);
        assert!((decayed[1] - 0.2 * DECAY_PER_FRAME).abs() < 1e-6);
    }

    #[test]
    fn mono_uses_one_channel_and_clamps() {
        let meter = Meter::default();
        meter.update(&frame(1, vec![0.3, -1.5]));
        assert_eq!(meter.levels(1), vec![1.0]);
        assert_eq!(meter.levels(2)[1], 0.0);
    }
}
