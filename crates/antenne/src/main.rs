//! Antenne — point d'entrée.

mod app;
mod cards;
mod cli;

use antenne_core::{Config, SourceConfig};
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Run => app::run(Config::load(&cli.config)?, cli.config.clone()).await,
        Command::Check => {
            let config = Config::load(&cli.config)?;
            print_summary(&config);
            Ok(())
        }
        Command::Devices => {
            print_devices();
            Ok(())
        }
    }
}

fn print_summary(config: &Config) {
    println!("Configuration valide — serveur sur {}", config.server.bind);
    println!(
        "Audio : {} Hz, trames de {} ms",
        config.audio.sample_rate, config.audio.frame_ms
    );
    println!();
    let header = ["ID", "NOM", "CANAUX", "SOURCE", "CODEC", "CHEMIN WHEP"];
    println!(
        "{:<12} {:<24} {:<7} {:<10} {:<7} {}",
        header[0], header[1], header[2], header[3], header[4], header[5]
    );
    for card in &config.cards {
        let source = match &card.source {
            SourceConfig::Virtual => "virtuelle",
            SourceConfig::Device { .. } => "entrée",
            SourceConfig::Tone { .. } => "tonalité",
        };
        let id = if card.enabled {
            card.id.clone()
        } else {
            format!("{} (off)", card.id)
        };
        println!(
            "{:<12} {:<24} {:<7} {:<10} {:<7} {}",
            id,
            card.name,
            card.channels.count(),
            source,
            card.encoding.label(),
            card.whep_path()
        );
    }
}

fn print_devices() {
    let devices = antenne_audio::devices::list_input_devices();
    if devices.is_empty() {
        println!("Aucune entrée audio détectée.");
        return;
    }
    let header = ["HÔTE", "CANAUX", "HZ", "FORMAT", "NOM"];
    println!(
        "{:<10} {:>6} {:>7} {:<6} {}",
        header[0], header[1], header[2], header[3], header[4]
    );
    for d in devices {
        println!(
            "{:<10} {:>6} {:>7} {:<6} {}",
            d.host,
            d.channels.map_or("?".into(), |c| c.to_string()),
            d.sample_rate.map_or("?".into(), |r| r.to_string()),
            d.sample_format.unwrap_or_else(|| "?".into()),
            d.name
        );
    }
}
