//! Cycle de vie des cartes : démarrage, modification et suppression à chaud.
//!
//! Le [`CardManager`] possède tout ce qui touche à l'audio (backend de cartes
//! virtuelles, sources en cours) et publie l'état des cartes dans la liste
//! partagée lue par le serveur HTTP. Chaque modification est validée sur la
//! configuration complète, écrite dans le fichier TOML, puis appliquée.
//!
//! Toutes les méthodes sont bloquantes : à appeler hors des threads tokio
//! (`spawn_blocking`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, RwLock};

use antenne_audio::capture::{self, CaptureSpec};
use antenne_audio::virtual_device::{self, VirtualBackend, VirtualCard};
use antenne_audio::{FrameSink, SourceHandle, tone};
use antenne_core::{AudioConfig, AudioFrame, CardConfig, Config, SourceConfig};
use antenne_net::{CardChannel, CardControl, CardStatus, ControlError, Meter, SharedCards};
use anyhow::anyhow;
use tokio::runtime::Handle;
use tokio::sync::broadcast;

/// Trames en attente par auditeur avant qu'il soit considéré « en retard »
/// (64 × 10 ms = 640 ms).
const FRAME_QUEUE: usize = 64;

/// Ressources audio d'une carte démarrée.
#[derive(Debug, Default)]
struct Running {
    source: Option<SourceHandle>,
    vcard: Option<VirtualCard>,
}

#[derive(Debug)]
pub struct CardManager {
    config: Config,
    config_path: PathBuf,
    backend: Option<Box<dyn VirtualBackend>>,
    running: HashMap<String, Running>,
    shared: SharedCards,
    rt: Handle,
}

impl CardManager {
    /// Crée le backend de cartes virtuelles et démarre toutes les cartes.
    pub fn start(config: Config, config_path: PathBuf, rt: Handle) -> Self {
        let mut manager = Self {
            backend: None,
            config,
            config_path,
            running: HashMap::new(),
            shared: Arc::new(RwLock::new(Vec::new())),
            rt,
        };
        manager.backend = manager.make_backend();
        manager.start_all();
        manager
    }

    pub fn shared(&self) -> SharedCards {
        Arc::clone(&self.shared)
    }

    pub fn backend_name(&self) -> Option<String> {
        self.backend.as_ref().map(|b| b.name().to_string())
    }

    fn make_backend(&self) -> Option<Box<dyn VirtualBackend>> {
        let needs_virtual = self
            .config
            .cards
            .iter()
            .any(|c| matches!(c.source, SourceConfig::Virtual));
        match virtual_device::make_backend(&self.config.platform) {
            Ok(b) => {
                tracing::info!(backend = b.name(), "backend de cartes virtuelles prêt");
                Some(b)
            }
            Err(e) if needs_virtual => {
                tracing::error!("cartes virtuelles indisponibles : {e:#}");
                None
            }
            Err(e) => {
                tracing::debug!("pas de backend de cartes virtuelles : {e:#}");
                None
            }
        }
    }

    // ---------- Démarrage / arrêt d'une carte ----------

    fn launch(&mut self, card: &CardConfig) -> Arc<CardChannel> {
        let (tx, _) = broadcast::channel::<AudioFrame>(FRAME_QUEUE);
        let status = if card.enabled {
            let mut running = Running::default();
            let status = match self.start_source(card, tx.clone(), &mut running) {
                Ok(handle) => {
                    let status = CardStatus::Running {
                        source: handle.label().to_string(),
                    };
                    running.source = Some(handle);
                    status
                }
                Err(e) => {
                    tracing::error!(card = %card.id, "{e:#}");
                    CardStatus::Failed {
                        error: format!("{e:#}"),
                    }
                }
            };
            // Même en échec, une carte virtuelle créée doit être supprimée plus tard.
            self.running.insert(card.id.clone(), running);
            status
        } else {
            CardStatus::Disabled
        };

        // L'encodeur n'est utilisé que par WebRTC (jalon 3) : on prévient tôt.
        if let Err(e) = antenne_codec::make_encoder(
            &card.encoding,
            card.channels,
            self.config.audio.sample_rate,
        ) {
            tracing::warn!(card = %card.id, "encodage {} indisponible : {e}", card.encoding.label());
        }

        let channel = Arc::new(CardChannel {
            config: card.clone(),
            status,
            frames: tx,
            listeners: Arc::new(AtomicUsize::new(0)),
            meter: Arc::new(Meter::default()),
        });
        self.rt.spawn(antenne_net::meter::run(
            Arc::clone(&channel.meter),
            channel.frames.subscribe(),
        ));
        channel
    }

    fn start_source(
        &mut self,
        card: &CardConfig,
        tx: broadcast::Sender<AudioFrame>,
        running: &mut Running,
    ) -> anyhow::Result<SourceHandle> {
        // `send` échoue s'il n'y a aucun auditeur : c'est normal, on ignore.
        let sink: FrameSink = Box::new(move |frame| {
            let _ = tx.send(frame);
        });
        let audio = &self.config.audio;

        match &card.source {
            SourceConfig::Tone { frequency_hz } => {
                tone::spawn_tone(&card.id, *frequency_hz, card.channels.count(), audio, sink)
            }
            SourceConfig::Device {
                device,
                first_channel,
            } => capture::spawn_capture(
                capture_spec(card, audio, vec![device.clone()], *first_channel),
                sink,
            ),
            SourceConfig::Virtual => {
                let backend = self
                    .backend
                    .as_mut()
                    .ok_or_else(|| anyhow!("aucun backend de cartes virtuelles disponible"))?;
                let vcard = backend.create(card, audio.sample_rate)?;
                let spec = capture_spec(
                    card,
                    audio,
                    vcard.capture_patterns.clone(),
                    vcard.first_channel,
                );
                running.vcard = Some(vcard);
                capture::spawn_capture(spec, sink)
            }
        }
    }

    /// Arrête la source puis supprime la carte virtuelle (dans cet ordre).
    fn halt(&mut self, id: &str) {
        let Some(running) = self.running.remove(id) else {
            return;
        };
        drop(running.source);
        if let (Some(vcard), Some(backend)) = (running.vcard, self.backend.as_mut()) {
            if let Err(e) = backend.destroy(&vcard) {
                tracing::warn!(card = %id, "suppression de la carte virtuelle : {e:#}");
            }
        }
    }

    fn start_all(&mut self) {
        let cards = self.config.cards.clone();
        let channels: Vec<_> = cards.iter().map(|c| self.launch(c)).collect();
        *write(&self.shared) = channels;
    }

    fn halt_all(&mut self) {
        let ids: Vec<String> = self.running.keys().cloned().collect();
        for id in ids {
            self.halt(&id);
        }
    }

    /// Recrée toutes les cartes avec un backend neuf (backends positionnels).
    fn restart_all(&mut self) {
        tracing::info!("backend positionnel : toutes les cartes sont recréées");
        self.halt_all();
        self.backend = self.make_backend();
        self.start_all();
    }

    fn positional(&self) -> bool {
        self.backend.as_ref().is_some_and(|b| b.positional())
    }

    /// Arrêt du programme : libère l'audio et ferme les flux en cours.
    pub fn shutdown(&mut self) {
        self.halt_all();
        // Les `Sender` disparaissent avec les cartes : les flux WAV se terminent,
        // ce qui laisse le serveur HTTP s'arrêter proprement.
        write(&self.shared).clear();
    }

    // ---------- Configuration ----------

    /// Valide `next`, l'écrit sur disque, puis l'adopte.
    fn commit(&mut self, next: Config) -> Result<(), ControlError> {
        next.validate()
            .map_err(|e| ControlError::Invalid(e.messages()))?;
        self.save(&next)?;
        self.config = next;
        Ok(())
    }

    fn save(&self, config: &Config) -> Result<(), ControlError> {
        let path = &self.config_path;
        let failed = |what: &str, e: &dyn std::fmt::Display| {
            ControlError::Failed(format!("{what} {} : {e}", path.display()))
        };
        let original = std::fs::read_to_string(path).unwrap_or_default();
        let text = config
            .rewrite_cards(&original)
            .map_err(|e| failed("préparation de", &e))?;
        if !original.is_empty() {
            let backup = path.with_extension("toml.bak");
            std::fs::write(&backup, &original).map_err(|e| failed("sauvegarde de", &e))?;
        }
        // Écriture atomique : fichier temporaire puis renommage.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(|e| failed("écriture de", &e))?;
        std::fs::rename(&tmp, path).map_err(|e| failed("écriture de", &e))?;
        tracing::info!(file = %path.display(), "configuration enregistrée");
        Ok(())
    }

    fn index_of(&self, id: &str) -> Result<usize, ControlError> {
        self.config
            .cards
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| ControlError::NotFound(id.to_string()))
    }

    // ---------- Opérations exposées à l'API ----------

    pub fn create(&mut self, card: CardConfig) -> Result<(), ControlError> {
        let mut next = self.config.clone();
        next.cards.push(card.clone());
        self.commit(next)?;
        if self.backend.is_none() && matches!(card.source, SourceConfig::Virtual) {
            self.backend = self.make_backend();
        }
        // Ajoutée en dernier : même un backend positionnel la place correctement.
        let channel = self.launch(&card);
        write(&self.shared).push(channel);
        tracing::info!(card = %card.id, "carte ajoutée");
        Ok(())
    }

    pub fn update(&mut self, id: &str, card: CardConfig) -> Result<(), ControlError> {
        let index = self.index_of(id)?;
        let old = self.config.cards[index].clone();
        let mut next = self.config.clone();
        next.cards[index] = card.clone();
        self.commit(next)?;

        if !needs_restart(&old, &card) {
            // Réglages sans effet sur la capture : on garde le flux, les
            // auditeurs et le vu-mètre.
            let mut cards = write(&self.shared);
            if let Some(slot) = cards.iter_mut().find(|c| c.config.id == id) {
                *slot = Arc::new(CardChannel {
                    config: card,
                    status: slot.status.clone(),
                    frames: slot.frames.clone(),
                    listeners: Arc::clone(&slot.listeners),
                    meter: Arc::clone(&slot.meter),
                });
            }
            tracing::info!(card = %id, "réglages modifiés (sans redémarrage)");
            return Ok(());
        }

        let touches_virtual = matches!(old.source, SourceConfig::Virtual)
            || matches!(card.source, SourceConfig::Virtual);
        if touches_virtual && self.positional() {
            self.restart_all();
        } else {
            if self.backend.is_none() && matches!(card.source, SourceConfig::Virtual) {
                self.backend = self.make_backend();
            }
            self.halt(id);
            let channel = self.launch(&card);
            let mut cards = write(&self.shared);
            if let Some(slot) = cards.iter_mut().find(|c| c.config.id == id) {
                *slot = channel;
            }
        }
        tracing::info!(card = %id, "carte redémarrée avec ses nouveaux réglages");
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<(), ControlError> {
        let index = self.index_of(id)?;
        let mut next = self.config.clone();
        let removed = next.cards.remove(index);
        self.commit(next)?;
        if matches!(removed.source, SourceConfig::Virtual) && self.positional() {
            self.restart_all();
        } else {
            self.halt(id);
            write(&self.shared).retain(|c| c.config.id != id);
        }
        tracing::info!(card = %id, "carte supprimée");
        Ok(())
    }
}

/// Faut-il recréer la source (et la carte virtuelle) pour appliquer `new` ?
fn needs_restart(old: &CardConfig, new: &CardConfig) -> bool {
    old.enabled != new.enabled
        || old.channels != new.channels
        || old.source != new.source
        // Le nom est celui de la sortie audio côté OS.
        || (matches!(new.source, SourceConfig::Virtual) && old.name != new.name)
}

fn write(shared: &SharedCards) -> std::sync::RwLockWriteGuard<'_, Vec<Arc<CardChannel>>> {
    shared
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn capture_spec(
    card: &CardConfig,
    audio: &AudioConfig,
    device_patterns: Vec<String>,
    first_channel: u16,
) -> CaptureSpec {
    let channels = card.channels.count();
    let ring_frames = audio.sample_rate as usize * audio.ring_buffer_ms as usize / 1000;
    CaptureSpec {
        card_id: card.id.clone(),
        device_patterns,
        first_channel,
        channels,
        sample_rate: audio.sample_rate,
        samples_per_frame: audio.samples_per_frame(),
        ring_capacity: ring_frames * usize::from(channels),
    }
}

/// Accès partagé au gestionnaire, pour le serveur HTTP. Les opérations sont
/// sérialisées : une seule modification de carte à la fois.
#[derive(Debug, Clone)]
pub struct ManagerHandle(Arc<Mutex<CardManager>>);

impl ManagerHandle {
    pub fn new(manager: CardManager) -> Self {
        Self(Arc::new(Mutex::new(manager)))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CardManager> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn shutdown(&self) {
        self.lock().shutdown();
    }
}

impl CardControl for ManagerHandle {
    fn create(&self, card: CardConfig) -> Result<(), ControlError> {
        self.lock().create(card)
    }

    fn update(&self, id: &str, card: CardConfig) -> Result<(), ControlError> {
        self.lock().update(id, card)
    }

    fn delete(&self, id: &str) -> Result<(), ControlError> {
        self.lock().delete(id)
    }

    fn input_devices(&self) -> Vec<String> {
        antenne_audio::devices::default_host_input_names()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antenne_core::{Channels, Encoding};

    fn card() -> CardConfig {
        CardConfig {
            id: "a".into(),
            name: "A".into(),
            enabled: true,
            channels: Channels::Stereo,
            source: SourceConfig::Virtual,
            encoding: Encoding::default(),
            path: None,
            max_listeners: 4,
        }
    }

    #[test]
    fn encoding_path_and_listeners_do_not_restart() {
        let old = card();
        let mut new = card();
        new.encoding = Encoding::Opus {
            bitrate_kbps: 32,
            fec: true,
        };
        new.path = Some("/radio/a".into());
        new.max_listeners = 1;
        assert!(!needs_restart(&old, &new));
    }

    #[test]
    fn audio_changes_restart() {
        let old = card();
        let mut mono = card();
        mono.channels = Channels::Mono;
        assert!(needs_restart(&old, &mono));

        let mut renamed = card();
        renamed.name = "B".into();
        assert!(needs_restart(&old, &renamed), "nom de la sortie virtuelle");

        let mut off = card();
        off.enabled = false;
        assert!(needs_restart(&old, &off));

        let mut tone = card();
        tone.source = SourceConfig::Tone {
            frequency_hz: 440.0,
        };
        assert!(needs_restart(&old, &tone));
        // Une tonalité renommée garde son flux.
        let mut tone_renamed = tone.clone();
        tone_renamed.name = "Autre".into();
        assert!(!needs_restart(&tone, &tone_renamed));
    }
}
