//! Noyau d'Antenne : configuration, types partagés et validation.
//!
//! Cette crate ne dépend d'aucune API audio, réseau ou OS : elle doit compiler
//! et se tester partout, très vite. Tout ce qui est spécifique à une plateforme
//! vit dans `antenne-audio`.

pub mod config;
pub mod frame;

pub use config::{
    AudioConfig, CardConfig, Channels, Config, ConfigError, Encoding, PlatformConfig, ServerConfig,
    SourceConfig, VirtualBackendKind,
};
pub use frame::AudioFrame;
