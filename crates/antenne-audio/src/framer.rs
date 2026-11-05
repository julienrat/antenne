//! Découpe un flux d'échantillons entrelacés en trames de durée fixe.

use std::sync::Arc;
use std::time::Instant;

use antenne_core::AudioFrame;

#[derive(Debug)]
pub(crate) struct Framer {
    sample_rate: u32,
    channels: u16,
    frame_len: usize,
    buf: Vec<f32>,
    seq: u64,
}

impl Framer {
    /// `samples_per_frame` est exprimé par canal.
    pub(crate) fn new(sample_rate: u32, channels: u16, samples_per_frame: usize) -> Self {
        let frame_len = samples_per_frame * usize::from(channels);
        Self {
            sample_rate,
            channels,
            frame_len,
            buf: Vec::with_capacity(frame_len),
            seq: 0,
        }
    }

    /// Ajoute un échantillon ; renvoie une trame quand elle est complète.
    #[inline]
    pub(crate) fn push(&mut self, sample: f32) -> Option<AudioFrame> {
        self.buf.push(sample);
        if self.buf.len() < self.frame_len {
            return None;
        }
        let frame = AudioFrame {
            seq: self.seq,
            sample_rate: self.sample_rate,
            channels: self.channels,
            samples: Arc::from(&self.buf[..]),
            captured_at: Instant::now(),
        };
        self.seq += 1;
        self.buf.clear();
        Some(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_full_frames_in_order() {
        let mut framer = Framer::new(48_000, 2, 480);
        let mut frames = Vec::new();
        for i in 0..(960 * 3 + 10) {
            if let Some(f) = framer.push(i as f32) {
                frames.push(f);
            }
        }
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].samples.len(), 960);
        assert_eq!(frames[1].seq, 1);
        assert_eq!(frames[2].samples[0], 1920.0);
        assert_eq!(frames[2].duration_us(), 10_000);
    }
}
