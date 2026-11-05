//! Orchestration : cartes (audio) d'un côté, serveur HTTP de l'autre.

use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

    // Le port d'abord : s'il est pris, inutile de créer les cartes virtuelles.
    let listener = match tokio::net::TcpListener::bind(bind).await {
        Ok(listener) => listener,
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            return Err(port_in_use(bind, &config_path).await);
        }
        Err(e) => return Err(anyhow!("serveur HTTP sur {bind} : {e}")),
    };

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
    let served = antenne_net::serve(listener, state, shutdown).await;

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

/// Erreur claire quand le port est pris : une autre Antenne, ou un autre programme ?
async fn port_in_use(bind: SocketAddr, config_path: &Path) -> anyhow::Error {
    let port = bind.port();
    match running_antenne(bind).await {
        Some(version) => anyhow!(
            "Antenne {version} tourne déjà sur le port {port} : son interface est \
             http://localhost:{port}/ (bouton « Quitter » pour l'arrêter)"
        ),
        None => anyhow!(
            "le port {port} est déjà utilisé par un autre programme : changer `bind` dans la \
             section [server] de {} (par exemple \"0.0.0.0:8090\")",
            config_path.display()
        ),
    }
}

/// Version de l'Antenne qui répond sur `bind`, s'il s'agit bien d'Antenne.
async fn running_antenne(bind: SocketAddr) -> Option<String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let ip = match bind.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    let probe = async {
        let mut stream = tokio::net::TcpStream::connect((ip, bind.port()))
            .await
            .ok()?;
        stream
            .write_all(b"GET /api/status HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .ok()?;
        let mut response = Vec::new();
        stream
            .take(16 * 1024)
            .read_to_end(&mut response)
            .await
            .ok()?;
        antenne_version(&String::from_utf8_lossy(&response))
    };
    tokio::time::timeout(Duration::from_secs(2), probe)
        .await
        .ok()
        .flatten()
}

/// Extrait `version` d'une réponse HTTP de `/api/status` d'Antenne.
fn antenne_version(response: &str) -> Option<String> {
    let body = response.split_once("\r\n\r\n")?.1;
    if !body.contains("\"virtual_backend\"") || !body.contains("\"sample_rate\"") {
        return None;
    }
    let start = body.find("\"version\":\"")? + "\"version\":\"".len();
    let len = body[start..].find('"')?;
    Some(body[start..start + len].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_antenne_status() {
        let ok = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n\
                  {\"version\":\"0.1.2\",\"sample_rate\":48000,\"virtual_backend\":null}";
        assert_eq!(antenne_version(ok).as_deref(), Some("0.1.2"));
        let other = "HTTP/1.1 200 OK\r\n\r\n{\"version\":\"9\"}";
        assert_eq!(antenne_version(other), None);
        assert_eq!(antenne_version("garbage"), None);
    }

    #[tokio::test]
    async fn port_in_use_names_other_programs() {
        // Un « programme » quelconque qui accepte puis ferme sans répondre.
        let busy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = busy.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((socket, _)) = busy.accept().await {
                drop(socket);
            }
        });
        let message = port_in_use(addr, Path::new("antenne.toml"))
            .await
            .to_string();
        assert!(message.contains("autre programme"), "{message}");
        assert!(message.contains("antenne.toml"), "{message}");
    }
}
