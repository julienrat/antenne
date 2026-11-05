//! Orchestration : cartes (audio) d'un côté, serveur HTTP de l'autre.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use antenne_core::Config;
use antenne_net::AppState;
use anyhow::{Context, anyhow};
use tokio::runtime::Handle;

use crate::cards::{CardManager, ManagerHandle};

pub async fn run(config: Config, config_path: PathBuf) -> anyhow::Result<()> {
    let bind = config.server.bind;
    let public_ip = config.server.public_ip;
    let audio = config.audio.clone();
    let quit = Arc::new(tokio::sync::Notify::new());

    // Création des cartes virtuelles et ouverture des périphériques : bloquant.
    let rt = Handle::current();
    let manager = tokio::task::spawn_blocking(move || CardManager::start(config, config_path, rt))
        .await
        .context("démarrage des cartes")?;
    let state_cards = manager.shared();
    let virtual_backend = manager.backend_name();
    let manager = ManagerHandle::new(manager);

    let state = AppState {
        sample_rate: audio.sample_rate,
        frame_ms: audio.frame_ms,
        virtual_backend,
        started_at: Instant::now(),
        cards: state_cards,
        control: Arc::new(manager.clone()),
        public_ip,
        sessions: Default::default(),
        quit: Arc::clone(&quit),
    };

    let on_signal = manager.clone();
    let shutdown = async move {
        // Ctrl+C dans le terminal, ou bouton « Quitter » de l'interface.
        tokio::select! {
            () = shutdown_signal() => {}
            () = quit.notified() => tracing::info!("arrêt demandé (interface web)"),
        }
        // Libère l'audio et ferme les flux en cours, sinon l'arrêt « propre »
        // du serveur attendrait indéfiniment les auditeurs connectés.
        let _ = tokio::task::spawn_blocking(move || on_signal.shutdown()).await;
    };
    let served = antenne_net::serve(bind, state, shutdown).await;

    // Si le serveur n'a pas pu démarrer (port pris…), on libère quand même tout.
    let _ = tokio::task::spawn_blocking(move || manager.shutdown()).await;

    served.map_err(|e| anyhow!("serveur HTTP sur {bind} : {e}"))
}

async fn shutdown_signal() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::error!("écoute de Ctrl+C impossible : {e}");
        std::future::pending::<()>().await;
    }
    tracing::info!("arrêt demandé");
}
