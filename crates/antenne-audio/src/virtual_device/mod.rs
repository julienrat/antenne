//! Cartes son virtuelles.
//!
//! | OS      | Backend par défaut | Mécanisme                                          |
//! |---------|--------------------|----------------------------------------------------|
//! | Linux   | `pipewire`         | un null sink PipeWire par carte (créé / supprimé)  |
//! | macOS   | `loopback`         | canaux d'un pilote existant (BlackHole…)           |
//! | Windows | `loopback`         | canaux d'un pilote existant (VB-CABLE, VAC…)       |
//!
//! Sous macOS et Windows, créer un vrai périphérique à la volée demande un
//! pilote (AudioServerPlugIn / pilote noyau signé) : hors périmètre v1, voir
//! docs/03-plateformes.md.

use antenne_core::{CardConfig, PlatformConfig, VirtualBackendKind};

mod loopback;
#[cfg(target_os = "linux")]
mod pipewire;

/// Carte virtuelle prête à être capturée.
#[derive(Debug, Clone)]
pub struct VirtualCard {
    pub card_id: String,
    /// Noms candidats de l'entrée à capturer (voir `capture::find_input_device`).
    pub capture_patterns: Vec<String>,
    /// Premier canal à lire sur ce périphérique.
    pub first_channel: u16,
    /// Ressource propre au backend à libérer (ex. index de module PipeWire).
    /// `None` si la carte existait déjà ou n'appartient pas à Antenne.
    pub handle: Option<String>,
}

/// Crée et supprime les cartes virtuelles.
pub trait VirtualBackend: Send + std::fmt::Debug {
    fn name(&self) -> &'static str;
    /// Crée (ou réutilise) la carte. Idempotent.
    fn create(&mut self, card: &CardConfig, sample_rate: u32) -> anyhow::Result<VirtualCard>;
    /// Supprime ce qui a été créé par `create`. Sans effet si `handle` est `None`.
    fn destroy(&mut self, card: &VirtualCard) -> anyhow::Result<()>;
    /// Vrai si l'emplacement d'une carte dépend des cartes créées avant elle
    /// (ex. `loopback`, qui attribue les canaux dans l'ordre) : modifier une
    /// carte oblige alors à recréer toutes les cartes, avec un backend neuf.
    fn positional(&self) -> bool {
        false
    }
}

/// Sélectionne le backend selon la plateforme et la configuration.
pub fn make_backend(platform: &PlatformConfig) -> anyhow::Result<Box<dyn VirtualBackend>> {
    match platform.virtual_backend {
        VirtualBackendKind::Auto if cfg!(target_os = "linux") => pipewire_backend(),
        VirtualBackendKind::Pipewire => pipewire_backend(),
        VirtualBackendKind::Auto | VirtualBackendKind::Loopback => {
            let device = platform.loopback_device.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "platform.loopback_device doit indiquer le pilote de bouclage \
                     (ex. \"BlackHole 16ch\", \"CABLE Output\")"
                )
            })?;
            Ok(Box::new(loopback::LoopbackBackend::new(device)))
        }
    }
}

#[cfg(target_os = "linux")]
fn pipewire_backend() -> anyhow::Result<Box<dyn VirtualBackend>> {
    Ok(Box::new(pipewire::PipewireBackend::new()?))
}

#[cfg(not(target_os = "linux"))]
fn pipewire_backend() -> anyhow::Result<Box<dyn VirtualBackend>> {
    anyhow::bail!("le backend PipeWire n'existe que sous Linux (utiliser \"loopback\")")
}
