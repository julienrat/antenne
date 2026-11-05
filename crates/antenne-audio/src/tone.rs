//! Générateur sinusoïdal temps réel : tester le pipeline sans matériel.

use std::f32::consts::TAU;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use antenne_core::AudioConfig;

use crate::framer::Framer;
use crate::{FrameSink, SourceHandle, SourceStats};

/// Amplitude du signal de test (≈ -14 dBFS).
const AMPLITUDE: f32 = 0.2;

/// Démarre un générateur à `frequency_hz` sur `channels` canaux.
pub fn spawn_tone(
    card_id: &str,
    frequency_hz: f32,
    channels: u16,
    audio: &AudioConfig,
    mut sink: FrameSink,
) -> anyhow::Result<SourceHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(SourceStats::default());
    let label = format!("tonalité {frequency_hz} Hz");

    let sample_rate = audio.sample_rate;
    let samples_per_frame = audio.samples_per_frame();
    let frame_duration = Duration::from_millis(u64::from(audio.frame_ms));

    let thread = {
        let stop = stop.clone();
        let stats = stats.clone();
        std::thread::Builder::new()
            .name(format!("tone-{card_id}"))
            .spawn(move || {
                let mut framer = Framer::new(sample_rate, channels, samples_per_frame);
                let step = TAU * frequency_hz / sample_rate as f32;
                let mut phase = 0.0_f32;
                let mut deadline = Instant::now();

                while !stop.load(Ordering::Relaxed) {
                    for _ in 0..samples_per_frame {
                        let value = phase.sin() * AMPLITUDE;
                        phase = (phase + step) % TAU;
                        for _ in 0..channels {
                            if let Some(frame) = framer.push(value) {
                                stats.frames.fetch_add(1, Ordering::Relaxed);
                                sink(frame);
                            }
                        }
                    }

                    // Cadence sur une échéance absolue : pas de dérive cumulée.
                    deadline += frame_duration;
                    let now = Instant::now();
                    if deadline > now {
                        std::thread::sleep(deadline - now);
                    } else if now - deadline > frame_duration * 10 {
                        // Gros retard (machine en veille…) : on se recale.
                        deadline = now;
                    }
                }
            })?
    };

    Ok(SourceHandle::new(label, stop, stats, thread))
}
