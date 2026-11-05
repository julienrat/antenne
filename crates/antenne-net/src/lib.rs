//! Serveur HTTP d'Antenne.
//!
//! | Route                     | Rôle                                            |
//! |---------------------------|-------------------------------------------------|
//! | `GET /`                   | interface web (supervision et réglages)         |
//! | `GET /health`             | sonde de vie                                    |
//! | `GET /api/cards`          | état et réglages des cartes (JSON)              |
//! | `POST /api/cards`         | ajoute une carte                                |
//! | `PUT /api/cards/{id}`     | modifie une carte                               |
//! | `DELETE /api/cards/{id}`  | supprime une carte                              |
//! | `GET /api/devices`        | entrées audio utilisables comme source          |
//! | `GET /api/status`         | état du serveur : fréquence, trame, durée       |
//! | `GET /api/levels`         | niveaux crête par carte et par canal            |
//! | `GET /stream/{id}`        | flux WAV PCM16 continu, pour tester (VLC…)      |
//! | `POST <path>`             | WHEP : offre SDP → session WebRTC de la carte   |
//! | `DELETE <path>/<id>`      | WHEP : fin de session                           |
//!
//! Les cartes changent pendant que le serveur tourne : le serveur ne fait que
//! lire la liste partagée ([`SharedCards`]) ; les modifications passent par
//! [`CardControl`], implémenté par l'application (qui possède l'audio).

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use antenne_core::{AudioFrame, CardConfig};
use axum::Router;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::serve::ListenerExt;
use serde::Serialize;
use tokio::sync::broadcast;

mod api;
pub mod meter;
mod ui;
pub mod wav;
pub mod webrtc;

pub use meter::Meter;

/// État d'une carte.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CardStatus {
    Running {
        source: String,
    },
    Failed {
        error: String,
    },
    /// `enabled = false` : déclarée mais pas démarrée.
    Disabled,
}

/// Une carte vue par le serveur : sa config et le canal de diffusion de ses trames.
#[derive(Debug)]
pub struct CardChannel {
    pub config: CardConfig,
    pub status: CardStatus,
    pub frames: broadcast::Sender<AudioFrame>,
    pub listeners: Arc<AtomicUsize>,
    /// Niveaux crête (voir [`meter::run`]).
    pub meter: Arc<Meter>,
}

/// Liste des cartes, dans l'ordre du fichier de configuration. Remplacée
/// carte par carte par l'application ; jamais verrouillée à travers un `await`.
pub type SharedCards = Arc<RwLock<Vec<Arc<CardChannel>>>>;

/// Refus d'une modification de carte.
#[derive(Debug, Clone)]
pub enum ControlError {
    NotFound(String),
    /// Configuration refusée : un message par problème.
    Invalid(Vec<String>),
    /// La modification n'a pas pu être appliquée (écriture du fichier…).
    Failed(String),
}

/// Modifications de cartes, implémentées par l'application.
///
/// Les méthodes sont bloquantes (création de carte virtuelle, ouverture du
/// périphérique, écriture du fichier) : le serveur les appelle via
/// `spawn_blocking`. Une carte dont la source échoue est quand même créée,
/// avec le statut [`CardStatus::Failed`].
pub trait CardControl: Send + Sync + 'static {
    fn create(&self, card: CardConfig) -> Result<(), ControlError>;
    fn update(&self, id: &str, card: CardConfig) -> Result<(), ControlError>;
    fn delete(&self, id: &str) -> Result<(), ControlError>;
    /// Noms des entrées audio utilisables comme `source.device`.
    fn input_devices(&self) -> Vec<String>;
}

/// État partagé par tous les handlers.
#[derive(Clone)]
pub struct AppState {
    pub sample_rate: u32,
    pub frame_ms: u32,
    /// Backend de cartes virtuelles actif (`pipewire`, `loopback`…), s'il y en a un.
    pub virtual_backend: Option<String>,
    pub started_at: Instant,
    pub cards: SharedCards,
    pub control: Arc<dyn CardControl>,
    /// IP annoncée dans les candidats ICE (`server.public_ip`) ; sinon
    /// l'interface par laquelle on joint chaque client.
    pub public_ip: Option<IpAddr>,
    /// Sessions WebRTC en cours.
    pub sessions: webrtc::Sessions,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("sample_rate", &self.sample_rate)
            .field("frame_ms", &self.frame_ms)
            .field("virtual_backend", &self.virtual_backend)
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// Instantané de la liste des cartes (copie des `Arc`, pas des cartes).
    pub fn cards(&self) -> Vec<Arc<CardChannel>> {
        match self.cards.read() {
            Ok(cards) => cards.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn card(&self, id: &str) -> Option<Arc<CardChannel>> {
        self.cards().into_iter().find(|c| c.config.id == id)
    }
}

/// Construit le routeur.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(ui::index))
        .route("/health", get(|| async { "ok" }))
        .route("/api/cards", get(api::list_cards).post(api::create_card))
        .route(
            "/api/cards/{id}",
            put(api::update_card).delete(api::delete_card),
        )
        .route("/api/devices", get(api::devices))
        .route("/api/status", get(ui::status))
        .route("/api/levels", get(ui::levels))
        .route("/stream/{id}", get(wav::stream_wav))
        // Les chemins WHEP sont libres et changent avec les cartes : résolus ici.
        .fallback(whep_or_not_found)
        .with_state(state)
}

async fn whep_or_not_found(State(state): State<AppState>, request: Request) -> Response {
    let path = request.uri().path().to_string();
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0);
    let cards = state.cards();

    // `POST <path>` : nouvelle session.
    if let Some(card) = cards.iter().find(|c| c.config.whep_path() == path) {
        if request.method() != Method::POST {
            return (StatusCode::METHOD_NOT_ALLOWED, "WHEP : POST attendu").into_response();
        }
        let card = Arc::clone(card);
        let headers = request.headers().clone();
        let body = match axum::body::to_bytes(request.into_body(), 64 * 1024).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(_) => {
                return (StatusCode::PAYLOAD_TOO_LARGE, "offre SDP trop grande").into_response();
            }
        };
        return webrtc::whep_offer(&state, card, &headers, body, peer).await;
    }

    // `DELETE <path>/<id>` : fin de session.
    if let Some((base, session)) = path.rsplit_once('/') {
        if let Some(card) = cards.iter().find(|c| c.config.whep_path() == base) {
            if request.method() == Method::DELETE {
                return webrtc::whep_delete(&state, &card.config.id, session);
            }
            return (StatusCode::METHOD_NOT_ALLOWED, "WHEP : DELETE attendu").into_response();
        }
    }
    (StatusCode::NOT_FOUND, "introuvable").into_response()
}

/// Démarre le serveur HTTP et rend la main à l'arrêt (`shutdown` résolu).
pub async fn serve<F>(bind: SocketAddr, state: AppState, shutdown: F) -> std::io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    // TCP_NODELAY : les trames de 10 ms partent tout de suite au lieu d'être
    // regroupées par l'algorithme de Nagle (jusqu'à ~40 ms de retard).
    let listener = tokio::net::TcpListener::bind(bind).await?.tap_io(|tcp| {
        if let Err(e) = tcp.set_nodelay(true) {
            tracing::debug!("TCP_NODELAY impossible : {e}");
        }
    });
    tracing::info!("serveur HTTP : http://{bind}  (interface : /, API : /api/cards)");
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await
}

/// Compte un auditeur tant qu'il est vivant ; refuse au-delà de `max`.
#[derive(Debug)]
pub(crate) struct ListenerGuard(Arc<AtomicUsize>);

impl ListenerGuard {
    // `fetch_update` est renommé `try_update` dans les toolchains récentes, mais
    // `try_update` n'existe pas en 1.89 (rust-version du workspace).
    #[allow(deprecated)]
    pub(crate) fn acquire(counter: &Arc<AtomicUsize>, max: usize) -> Option<Self> {
        counter
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n < max).then_some(n + 1)
            })
            .ok()
            .map(|_| Self(counter.clone()))
    }
}

impl Drop for ListenerGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listener_guard_enforces_max() {
        let counter = Arc::new(AtomicUsize::new(0));
        let a = ListenerGuard::acquire(&counter, 2).unwrap();
        let _b = ListenerGuard::acquire(&counter, 2).unwrap();
        assert!(ListenerGuard::acquire(&counter, 2).is_none());
        drop(a);
        assert!(ListenerGuard::acquire(&counter, 2).is_some());
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
