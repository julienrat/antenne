//! Backend Linux : un null sink PipeWire par carte.
//!
//! v1 : pilotage via `pactl` (fourni par pipewire-pulse / pulseaudio-utils),
//! simple et robuste. Les applications jouent vers « Antenne Carte 1 » ; Antenne
//! capture le moniteur du sink (l'hôte PipeWire de cpal expose les sinks comme
//! entrées capturables).
//!
//! TODO(jalon 5) : créer les nœuds via libpipewire (crate `pipewire`) pour se
//! passer de `pactl` et régler finement la latence (`node.latency`).

use std::process::Command;

use antenne_core::{CardConfig, Channels};
use anyhow::{Context, bail};

use super::{VirtualBackend, VirtualCard};

#[derive(Debug)]
pub struct PipewireBackend;

impl PipewireBackend {
    pub fn new() -> anyhow::Result<Self> {
        let ok = Command::new("pactl")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        if !ok {
            bail!(
                "`pactl` introuvable : installer pipewire-pulse ou pulseaudio-utils \
                 (sudo apt install pulseaudio-utils)"
            );
        }
        Ok(Self)
    }
}

/// Nom technique du sink (`node.name`), stable entre deux lancements.
fn sink_name(card_id: &str) -> String {
    format!("antenne_{card_id}")
}

fn sink_exists(name: &str) -> anyhow::Result<bool> {
    let output = Command::new("pactl")
        .args(["list", "short", "sinks"])
        .output()
        .context("pactl list short sinks")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .any(|line| line.split('\t').nth(1) == Some(name)))
}

impl VirtualBackend for PipewireBackend {
    fn name(&self) -> &'static str {
        "pipewire"
    }

    fn create(&mut self, card: &CardConfig, sample_rate: u32) -> anyhow::Result<VirtualCard> {
        let sink = sink_name(&card.id);
        let patterns = vec![card.name.clone(), sink.clone()];

        if sink_exists(&sink)? {
            tracing::info!(card = %card.id, sink = %sink, "sink déjà présent, réutilisé");
            return Ok(VirtualCard {
                card_id: card.id.clone(),
                capture_patterns: patterns,
                first_channel: 0,
                handle: None,
            });
        }

        let channel_map = match card.channels {
            Channels::Mono => "mono",
            Channels::Stereo => "front-left,front-right",
        };
        // pipewire-pulse coupe la valeur au premier espace si elle n'est pas
        // encadrée de guillemets simples à l'extérieur et doubles à l'intérieur
        // (`sink_properties='device.description="…"'`) : on neutralise donc
        // les deux sortes de guillemets dans le nom.
        let description = card.name.replace(['"', '\''], "’");
        let output = Command::new("pactl")
            .args([
                "load-module".to_string(),
                "module-null-sink".to_string(),
                format!("sink_name={sink}"),
                format!("sink_properties='device.description=\"{description}\"'"),
                format!("channels={}", card.channels.count()),
                format!("channel_map={channel_map}"),
                format!("rate={sample_rate}"),
            ])
            .output()
            .context("exécution de pactl load-module")?;
        if !output.status.success() {
            bail!(
                "pactl load-module a échoué : {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let index = String::from_utf8_lossy(&output.stdout).trim().to_string();
        index
            .parse::<u32>()
            .with_context(|| format!("index de module inattendu : « {index} »"))?;

        tracing::info!(card = %card.id, sink = %sink, module = %index, "carte virtuelle créée");
        Ok(VirtualCard {
            card_id: card.id.clone(),
            capture_patterns: patterns,
            first_channel: 0,
            handle: Some(index),
        })
    }

    fn destroy(&mut self, card: &VirtualCard) -> anyhow::Result<()> {
        let Some(index) = &card.handle else {
            return Ok(());
        };
        let status = Command::new("pactl")
            .args(["unload-module", index.as_str()])
            .status()
            .context("exécution de pactl unload-module")?;
        if !status.success() {
            bail!("pactl unload-module {index} a échoué");
        }
        tracing::info!(card = %card.card_id, "carte virtuelle supprimée");
        Ok(())
    }
}
