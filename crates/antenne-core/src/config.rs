//! Configuration d'Antenne (fichier TOML).
//!
//! Voir `config/antenne.example.toml` pour un exemple commenté. Toute la
//! validation métier est faite ici, une fois, au chargement : le reste du
//! programme peut considérer la configuration comme cohérente.

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Erreurs de chargement ou de validation de la configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("lecture de {path} impossible : {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("TOML invalide : {0}")]
    Parse(#[from] toml::de::Error),
    #[error("configuration invalide :\n{}", .0.join("\n"))]
    Invalid(Vec<String>),
    #[error("écriture du TOML impossible : {0}")]
    Serialize(String),
}

impl ConfigError {
    /// Messages d'erreur un par un, sans l'en-tête ni les puces.
    pub fn messages(&self) -> Vec<String> {
        match self {
            ConfigError::Invalid(errors) => errors
                .iter()
                .map(|e| e.trim_start().trim_start_matches("- ").to_string())
                .collect(),
            other => vec![other.to_string()],
        }
    }
}

/// Racine de la configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub platform: PlatformConfig,
    /// Une entrée `[[card]]` par carte son virtuelle / flux diffusé.
    #[serde(default, rename = "card")]
    pub cards: Vec<CardConfig>,
}

/// Serveur HTTP (API, signalisation WebRTC, flux de debug).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ServerConfig {
    /// Adresse d'écoute, ex. `0.0.0.0:8080`.
    pub bind: SocketAddr,
    /// IP annoncée aux clients dans les candidats ICE (utile si la machine a
    /// plusieurs interfaces). `None` = détection automatique.
    pub public_ip: Option<IpAddr>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], 8080)),
            public_ip: None,
        }
    }
}

/// Paramètres audio globaux, communs à toutes les cartes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AudioConfig {
    /// Fréquence interne du pipeline (Hz). 48000 recommandé (natif Opus).
    pub sample_rate: u32,
    /// Durée d'une trame (ms). 10 ms = bon compromis latence / surcoût.
    pub frame_ms: u32,
    /// Capacité du tampon entre le callback audio et le reste du pipeline (ms).
    pub ring_buffer_ms: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            frame_ms: 10,
            ring_buffer_ms: 200,
        }
    }
}

impl AudioConfig {
    /// Nombre d'échantillons par canal dans une trame.
    pub fn samples_per_frame(&self) -> usize {
        (self.sample_rate as usize * self.frame_ms as usize) / 1000
    }
}

/// Comment créer les cartes `source = { kind = "virtual" }`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VirtualBackendKind {
    /// Linux : PipeWire. macOS / Windows : périphérique de bouclage.
    #[default]
    Auto,
    /// Linux : crée un « null sink » PipeWire par carte.
    Pipewire,
    /// Toutes plateformes : répartit les cartes sur les canaux d'un
    /// périphérique de bouclage existant (BlackHole, VB-CABLE…).
    Loopback,
}

/// Réglages propres à la plateforme.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PlatformConfig {
    pub virtual_backend: VirtualBackendKind,
    /// Nom (ou extrait du nom) du périphérique de bouclage multicanal utilisé
    /// par le backend `loopback`, ex. `"BlackHole 16ch"`.
    pub loopback_device: Option<String>,
}

/// Mono ou stéréo, par carte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channels {
    Mono,
    Stereo,
}

impl Channels {
    pub fn count(self) -> u16 {
        match self {
            Channels::Mono => 1,
            Channels::Stereo => 2,
        }
    }
}

/// Origine du son d'une carte.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceConfig {
    /// Carte son virtuelle créée / gérée par Antenne.
    #[default]
    Virtual,
    /// Périphérique d'entrée existant (micro, interface, bouclage…).
    Device {
        /// Nom ou extrait du nom, insensible à la casse (voir `antenne devices`).
        device: String,
        /// Premier canal utilisé (0 = premier canal du périphérique).
        #[serde(default)]
        first_channel: u16,
    },
    /// Générateur sinusoïdal, pour tester sans matériel.
    Tone {
        #[serde(default = "default_tone_hz")]
        frequency_hz: f32,
    },
}

/// Encodage utilisé pour la diffusion WebRTC de la carte.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "codec", rename_all = "snake_case")]
pub enum Encoding {
    /// Opus : recommandé (faible latence, bon rendu, natif WebRTC).
    Opus {
        #[serde(default = "default_opus_bitrate")]
        bitrate_kbps: u32,
        /// Correction d'erreurs intégrée (utile en Wi-Fi).
        #[serde(default)]
        fec: bool,
    },
    /// G.711 µ-law (PCMU), 8 kHz mono : très léger à décoder.
    G711u,
    /// G.711 A-law (PCMA), 8 kHz mono.
    G711a,
    /// PCM 16 bits brut (flux HTTP / RTP L16 uniquement, pas WebRTC standard).
    Pcm16,
}

impl Default for Encoding {
    fn default() -> Self {
        Encoding::Opus {
            bitrate_kbps: default_opus_bitrate(),
            fec: false,
        }
    }
}

impl Encoding {
    pub fn label(&self) -> &'static str {
        match self {
            Encoding::Opus { .. } => "opus",
            Encoding::G711u => "g711u",
            Encoding::G711a => "g711a",
            Encoding::Pcm16 => "pcm16",
        }
    }
}

/// Une carte son virtuelle = une entrée audio = un flux diffusé.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardConfig {
    /// Identifiant court : `[a-z0-9_-]`, 32 caractères max. Sert dans les URL.
    pub id: String,
    /// Nom affiché (et nom de la carte côté OS quand Antenne la crée).
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_channels")]
    pub channels: Channels,
    #[serde(default)]
    pub source: SourceConfig,
    #[serde(default)]
    pub encoding: Encoding,
    /// Chemin HTTP de signalisation WebRTC (WHEP). Défaut : `/whep/<id>`.
    #[serde(default)]
    pub path: Option<String>,
    /// Nombre maximal d'auditeurs simultanés sur ce flux.
    #[serde(default = "default_max_listeners")]
    pub max_listeners: usize,
}

impl CardConfig {
    /// Chemin WHEP effectif de la carte.
    pub fn whep_path(&self) -> String {
        self.path
            .clone()
            .unwrap_or_else(|| format!("/whep/{}", self.id))
    }
}

fn default_true() -> bool {
    true
}
fn default_channels() -> Channels {
    Channels::Stereo
}
fn default_tone_hz() -> f32 {
    440.0
}
fn default_opus_bitrate() -> u32 {
    96
}
fn default_max_listeners() -> usize {
    4
}

/// Chemins réservés par le serveur HTTP.
const RESERVED_PATHS: [&str; 3] = ["/health", "/api", "/stream"];

impl Config {
    /// Charge et valide un fichier de configuration.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_toml_str(&text)
    }

    /// Analyse et valide une configuration TOML.
    pub fn from_toml_str(text: &str) -> Result<Self, ConfigError> {
        let config: Config = toml::from_str(text)?;
        config.validate()?;
        Ok(config)
    }

    /// Réécrit le texte TOML `original` avec les cartes de `self`.
    ///
    /// Le reste du fichier (sections `[server]`, `[audio]`…, commentaires
    /// compris) est conservé tel quel. Les commentaires placés juste avant un
    /// `[[card]]` suivent la carte (même `id`) ; ceux à l'intérieur d'une carte
    /// sont perdus. Si `original` n'est pas du TOML lisible, le fichier est
    /// régénéré entièrement.
    pub fn rewrite_cards(&self, original: &str) -> Result<String, ConfigError> {
        use toml_edit::{ArrayOfTables, DocumentMut, Item, RawString, Value};

        #[derive(Serialize)]
        struct CardsOnly<'a> {
            card: &'a [CardConfig],
        }

        let Ok(mut doc) = original.parse::<DocumentMut>() else {
            return toml::to_string(self).map_err(|e| ConfigError::Serialize(e.to_string()));
        };

        let generated = toml::to_string(&CardsOnly { card: &self.cards })
            .map_err(|e| ConfigError::Serialize(e.to_string()))?;
        let generated: DocumentMut = generated
            .parse()
            .map_err(|e: toml_edit::TomlError| ConfigError::Serialize(e.to_string()))?;
        let mut cards = match generated.get("card") {
            Some(Item::ArrayOfTables(array)) => array.clone(),
            _ => ArrayOfTables::new(),
        };

        // Commentaires précédant chaque ancienne carte, par id (le premier sert
        // d'en-tête de section et reste en tête quoi qu'il arrive).
        let old: Vec<(Option<String>, Option<RawString>)> = doc
            .get("card")
            .and_then(Item::as_array_of_tables)
            .map(|array| {
                array
                    .iter()
                    .map(|t| {
                        let id = t.get("id").and_then(Item::as_str).map(str::to_owned);
                        (id, t.decor().prefix().cloned())
                    })
                    .collect()
            })
            .unwrap_or_default();

        for (index, table) in cards.iter_mut().enumerate() {
            for key in ["source", "encoding"] {
                if let Some(Item::Table(sub)) = table.get(key) {
                    let inline = sub.clone().into_inline_table();
                    table.insert(key, Item::Value(Value::InlineTable(inline)));
                }
            }
            let id = table.get("id").and_then(Item::as_str).map(str::to_owned);
            let prefix = if index == 0 {
                old.first().and_then(|(_, p)| p.clone())
            } else {
                old.iter()
                    .enumerate()
                    .find(|(i, (old_id, _))| *i > 0 && old_id.is_some() && *old_id == id)
                    .and_then(|(_, (_, p))| p.clone())
            };
            table
                .decor_mut()
                .set_prefix(prefix.unwrap_or_else(|| RawString::from("\n")));
            // Position libre : les cartes restent là où était `[[card]]`.
            table.set_position(None);
        }

        doc.insert("card", Item::ArrayOfTables(cards));
        Ok(doc.to_string())
    }

    /// Cartes actives (`enabled = true`).
    pub fn enabled_cards(&self) -> impl Iterator<Item = &CardConfig> {
        self.cards.iter().filter(|c| c.enabled)
    }

    /// Vérifie la cohérence globale. Renvoie *toutes* les erreurs d'un coup.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut errors = Vec::new();

        if ![8_000, 12_000, 16_000, 24_000, 48_000].contains(&self.audio.sample_rate) {
            errors.push(format!(
                "audio.sample_rate = {} : valeurs acceptées 8000, 12000, 16000, 24000, 48000",
                self.audio.sample_rate
            ));
        }
        if ![5, 10, 20, 40, 60].contains(&self.audio.frame_ms) {
            errors.push(format!(
                "audio.frame_ms = {} : valeurs acceptées 5, 10, 20, 40, 60",
                self.audio.frame_ms
            ));
        }
        if self.audio.ring_buffer_ms < self.audio.frame_ms * 2 {
            errors.push("audio.ring_buffer_ms doit valoir au moins 2 × frame_ms".to_string());
        }
        if self.cards.is_empty() {
            errors.push("aucune carte définie (section [[card]])".to_string());
        }

        let mut ids = HashSet::new();
        let mut paths = HashSet::new();
        for card in &self.cards {
            let label = format!("carte « {} »", card.id);

            if !is_valid_slug(&card.id) {
                errors.push(format!(
                    "{label} : id invalide (attendu [a-z0-9_-], 1 à 32 caractères)"
                ));
            }
            if !ids.insert(card.id.as_str()) {
                errors.push(format!("{label} : id en double"));
            }
            if card.name.trim().is_empty() {
                errors.push(format!("{label} : name ne doit pas être vide"));
            }

            let path = card.whep_path();
            if let Err(e) = validate_path(&path) {
                errors.push(format!("{label} : path « {path} » {e}"));
            }
            if !paths.insert(path.clone()) {
                errors.push(format!("{label} : path « {path} » déjà utilisé"));
            }

            match &card.encoding {
                Encoding::G711u | Encoding::G711a if card.channels == Channels::Stereo => {
                    errors.push(format!("{label} : G.711 est mono uniquement"));
                }
                Encoding::Opus { bitrate_kbps, .. } if !(6..=510).contains(bitrate_kbps) => {
                    errors.push(format!(
                        "{label} : bitrate Opus hors limites (6 à 510 kb/s)"
                    ));
                }
                _ => {}
            }

            if card.max_listeners == 0 {
                errors.push(format!("{label} : max_listeners doit être ≥ 1"));
            }
            match &card.source {
                SourceConfig::Tone { frequency_hz }
                    if !(20.0..=20_000.0).contains(frequency_hz) =>
                {
                    errors.push(format!("{label} : frequency_hz hors de 20..20000"));
                }
                SourceConfig::Device { device, .. } if device.trim().is_empty() => {
                    errors.push(format!("{label} : source.device ne doit pas être vide"));
                }
                _ => {}
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Invalid(
                errors.into_iter().map(|e| format!("  - {e}")).collect(),
            ))
        }
    }
}

fn is_valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn validate_path(path: &str) -> Result<(), String> {
    if !path.starts_with('/') || path.len() < 2 {
        return Err("doit commencer par / et ne pas être vide".to_string());
    }
    if path.contains(['{', '}', '*', '?', '#', ' ']) {
        return Err("contient un caractère interdit ({ } * ? # espace)".to_string());
    }
    for reserved in RESERVED_PATHS {
        if path == reserved || path.starts_with(&format!("{reserved}/")) {
            return Err(format!("est réservé ({reserved})"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../../../config/antenne.example.toml");

    #[test]
    fn example_config_is_valid() {
        let config = Config::from_toml_str(EXAMPLE).expect("l'exemple doit rester valide");
        assert!(!config.cards.is_empty());
    }

    #[test]
    fn minimal_card_gets_defaults() {
        let config = Config::from_toml_str(
            r#"
            [[card]]
            id = "c1"
            name = "Carte 1"
            "#,
        )
        .unwrap();
        let card = &config.cards[0];
        assert!(card.enabled);
        assert_eq!(card.channels, Channels::Stereo);
        assert_eq!(card.source, SourceConfig::Virtual);
        assert_eq!(card.encoding.label(), "opus");
        assert_eq!(card.whep_path(), "/whep/c1");
        assert_eq!(config.audio.samples_per_frame(), 480);
    }

    #[test]
    fn inline_tables_are_parsed() {
        let config = Config::from_toml_str(
            r#"
            [[card]]
            id = "salle"
            name = "Salle"
            channels = "mono"
            source = { kind = "device", device = "BlackHole 16ch", first_channel = 2 }
            encoding = { codec = "g711u" }
            path = "/radio/salle"
            "#,
        )
        .unwrap();
        let card = &config.cards[0];
        assert_eq!(
            card.source,
            SourceConfig::Device {
                device: "BlackHole 16ch".into(),
                first_channel: 2
            }
        );
        assert_eq!(card.encoding, Encoding::G711u);
        assert_eq!(card.whep_path(), "/radio/salle");
    }

    #[test]
    fn rejects_duplicates_reserved_paths_and_stereo_g711() {
        let err = Config::from_toml_str(
            r#"
            [[card]]
            id = "a"
            name = "A"
            path = "/api/a"

            [[card]]
            id = "a"
            name = "A bis"
            encoding = { codec = "g711a" }
            "#,
        )
        .unwrap_err();
        let errors = match err {
            ConfigError::Invalid(errors) => errors,
            other => panic!("erreur de validation attendue, obtenu : {other}"),
        };
        let all = errors.join("\n");
        assert!(all.contains("réservé"), "{all}");
        assert!(all.contains("id en double"), "{all}");
        assert!(all.contains("G.711 est mono"), "{all}");
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = Config::from_toml_str(
            r#"
            [[card]]
            id = "a"
            name = "A"
            chanels = "mono"
            "#,
        )
        .unwrap_err();
        assert!(matches!(err, ConfigError::Parse(_)));
    }

    #[test]
    fn rewrite_cards_keeps_other_sections_and_comments() {
        let mut config = Config::from_toml_str(EXAMPLE).unwrap();
        config.cards[0].channels = Channels::Mono;
        config.cards[0].name = "Plateau".into();
        config.cards.remove(1);
        config.cards.push(CardConfig {
            id: "nouvelle".into(),
            name: "Nouvelle".into(),
            enabled: false,
            channels: Channels::Mono,
            source: SourceConfig::Tone {
                frequency_hz: 880.0,
            },
            encoding: Encoding::G711a,
            path: Some("/radio/nouvelle".into()),
            max_listeners: 3,
        });

        let text = config.rewrite_cards(EXAMPLE).unwrap();
        // Commentaires hors cartes et en-tête de section conservés.
        assert!(text.contains("# Adresse d'écoute"), "{text}");
        assert!(text.contains("# --- Cartes"), "{text}");
        // Sous-tables écrites en tables en ligne, comme à la main.
        assert!(text.contains("source = { kind = \"tone\""), "{text}");
        assert!(!text.contains("[card.source]"), "{text}");
        // Les sections globales restent avant les cartes.
        assert!(text.find("[server]") < text.find("[[card]]"), "{text}");

        let reread = Config::from_toml_str(&text).unwrap();
        assert_eq!(reread.cards.len(), 3);
        assert_eq!(reread.cards[0].name, "Plateau");
        assert_eq!(reread.cards[0].channels, Channels::Mono);
        assert_eq!(
            reread.cards[2].source,
            SourceConfig::Tone {
                frequency_hz: 880.0
            }
        );
        assert_eq!(reread.cards[2].whep_path(), "/radio/nouvelle");
        assert!(!reread.cards[2].enabled);
        assert_eq!(reread.server.bind, config.server.bind);
    }

    #[test]
    fn error_messages_are_unbulleted() {
        let err = Config::from_toml_str("").unwrap_err();
        assert_eq!(
            err.messages(),
            vec!["aucune carte définie (section [[card]])"]
        );
    }

    #[test]
    fn rejects_empty_config() {
        assert!(matches!(
            Config::from_toml_str(""),
            Err(ConfigError::Invalid(_))
        ));
    }
}
