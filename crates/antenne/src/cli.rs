//! Ligne de commande.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "antenne",
    version,
    about = "Cartes son virtuelles + diffusion unicast faible latence vers des ESP32"
)]
pub struct Cli {
    /// Fichier de configuration TOML.
    #[arg(short, long, global = true, default_value = "config/antenne.toml")]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Crée les cartes, démarre la capture et le serveur (Ctrl+C pour arrêter).
    Run,
    /// Valide le fichier de configuration et affiche un résumé.
    Check,
    /// Liste les entrées audio visibles (pour remplir `source.device`).
    Devices,
}
