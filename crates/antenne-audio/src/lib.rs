//! Audio d'Antenne : sources (capture, tonalité de test) et cartes virtuelles.
//!
//! Modèle de threads (voir docs/02-architecture.md) :
//! callback cpal (temps réel) -> ring buffer `rtrb` -> thread « pompe » qui
//! découpe en trames et appelle le [`FrameSink`]. Le callback ne fait jamais
//! d'allocation, de verrou ni de log.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;

use antenne_core::AudioFrame;

pub mod capture;
pub mod devices;
mod framer;
pub mod tone;
pub mod virtual_device;

/// Destination des trames produites par une source (appelée sur le thread
/// pompe, jamais dans le callback audio).
pub type FrameSink = Box<dyn FnMut(AudioFrame) + Send + 'static>;

/// Compteurs d'une source, lisibles à tout moment.
#[derive(Debug, Default)]
pub struct SourceStats {
    /// Trames produites.
    pub frames: AtomicU64,
    /// Échantillons perdus car le ring buffer était plein.
    pub overruns: AtomicU64,
}

/// Source audio en cours d'exécution. L'arrêt se fait au `drop`.
#[derive(Debug)]
pub struct SourceHandle {
    label: String,
    stop: Arc<AtomicBool>,
    stats: Arc<SourceStats>,
    thread: Option<JoinHandle<()>>,
}

impl SourceHandle {
    pub(crate) fn new(
        label: String,
        stop: Arc<AtomicBool>,
        stats: Arc<SourceStats>,
        thread: JoinHandle<()>,
    ) -> Self {
        Self {
            label,
            stop,
            stats,
            thread: Some(thread),
        }
    }

    /// Description lisible (périphérique ou générateur).
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn stats(&self) -> &SourceStats {
        &self.stats
    }
}

impl Drop for SourceHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let panicked = self.thread.take().is_some_and(|t| t.join().is_err());
        if panicked {
            tracing::error!(source = %self.label, "le thread de la source a paniqué");
        }
    }
}
