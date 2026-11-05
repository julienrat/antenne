//! API JSON des cartes : lecture et modifications.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use antenne_core::{CardConfig, Channels, Encoding, SourceConfig};
use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::{AppState, CardChannel, CardControl, CardStatus, ControlError};

/// Une carte telle que l'interface la voit : réglages complets + état.
#[derive(Debug, Serialize)]
pub(crate) struct CardInfo {
    id: String,
    name: String,
    enabled: bool,
    channels: Channels,
    source: SourceConfig,
    encoding: Encoding,
    /// Chemin WHEP tel qu'écrit dans la config (`null` = défaut).
    path: Option<String>,
    /// Chemin WHEP effectif.
    whep_path: String,
    debug_stream: String,
    listeners: usize,
    max_listeners: usize,
    status: CardStatus,
}

impl From<&CardChannel> for CardInfo {
    fn from(c: &CardChannel) -> Self {
        Self {
            id: c.config.id.clone(),
            name: c.config.name.clone(),
            enabled: c.config.enabled,
            channels: c.config.channels,
            source: c.config.source.clone(),
            encoding: c.config.encoding.clone(),
            path: c.config.path.clone(),
            whep_path: c.config.whep_path(),
            debug_stream: format!("/stream/{}", c.config.id),
            listeners: c.listeners.load(Ordering::Relaxed),
            max_listeners: c.config.max_listeners,
            status: c.status.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    /// Résumé lisible (premier problème, ou tous joints).
    error: String,
    /// Un message par problème.
    errors: Vec<String>,
}

fn error(status: StatusCode, errors: Vec<String>) -> Response {
    let body = ErrorBody {
        error: errors.join(" ; "),
        errors,
    };
    (status, Json(body)).into_response()
}

impl IntoResponse for ControlError {
    fn into_response(self) -> Response {
        match self {
            ControlError::NotFound(id) => error(
                StatusCode::NOT_FOUND,
                vec![format!("carte « {id} » inconnue")],
            ),
            ControlError::Invalid(errors) => error(StatusCode::BAD_REQUEST, errors),
            ControlError::Failed(e) => error(StatusCode::INTERNAL_SERVER_ERROR, vec![e]),
        }
    }
}

fn bad_json(rejection: JsonRejection) -> Response {
    error(
        StatusCode::BAD_REQUEST,
        vec![format!("requête invalide : {}", rejection.body_text())],
    )
}

/// Exécute une modification bloquante hors des threads asynchrones.
async fn run_blocking<F>(state: &AppState, f: F) -> Result<(), ControlError>
where
    F: FnOnce(&dyn CardControl) -> Result<(), ControlError> + Send + 'static,
{
    let control = Arc::clone(&state.control);
    tokio::task::spawn_blocking(move || f(control.as_ref()))
        .await
        .unwrap_or_else(|e| Err(ControlError::Failed(format!("tâche interrompue : {e}"))))
}

fn card_response(state: &AppState, id: &str, status: StatusCode) -> Response {
    match state.card(id) {
        Some(card) => (status, Json(CardInfo::from(card.as_ref()))).into_response(),
        None => ControlError::NotFound(id.to_string()).into_response(),
    }
}

/// `GET /api/cards`
pub(crate) async fn list_cards(State(state): State<AppState>) -> Json<Vec<CardInfo>> {
    Json(
        state
            .cards()
            .iter()
            .map(|c| CardInfo::from(c.as_ref()))
            .collect(),
    )
}

/// `POST /api/cards`
pub(crate) async fn create_card(
    State(state): State<AppState>,
    body: Result<Json<CardConfig>, JsonRejection>,
) -> Response {
    let card = match body {
        Ok(Json(card)) => card,
        Err(rejection) => return bad_json(rejection),
    };
    let id = card.id.clone();
    match run_blocking(&state, move |control| control.create(card)).await {
        Ok(()) => card_response(&state, &id, StatusCode::CREATED),
        Err(e) => e.into_response(),
    }
}

/// `PUT /api/cards/{id}` : remplace les réglages (l'identifiant ne change pas).
pub(crate) async fn update_card(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<CardConfig>, JsonRejection>,
) -> Response {
    let card = match body {
        Ok(Json(card)) => card,
        Err(rejection) => return bad_json(rejection),
    };
    if card.id != id {
        return error(
            StatusCode::BAD_REQUEST,
            vec!["l'identifiant d'une carte ne peut pas être modifié".to_string()],
        );
    }
    let target = id.clone();
    match run_blocking(&state, move |control| control.update(&target, card)).await {
        Ok(()) => card_response(&state, &id, StatusCode::OK),
        Err(e) => e.into_response(),
    }
}

/// `DELETE /api/cards/{id}`
pub(crate) async fn delete_card(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match run_blocking(&state, move |control| control.delete(&id)).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => e.into_response(),
    }
}

/// `GET /api/devices`
pub(crate) async fn devices(State(state): State<AppState>) -> Response {
    let control = Arc::clone(&state.control);
    match tokio::task::spawn_blocking(move || control.input_devices()).await {
        Ok(names) => Json(names).into_response(),
        Err(e) => ControlError::Failed(format!("tâche interrompue : {e}")).into_response(),
    }
}

/// `POST /api/shutdown` : arrêt d'Antenne demandé depuis l'interface.
///
/// La réponse part avant l'arrêt ; l'application libère ensuite l'audio
/// (cartes virtuelles supprimées) et ferme les flux, comme pour Ctrl+C.
pub(crate) async fn shutdown(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Response {
    let client = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|c| c.0.to_string())
        .unwrap_or_else(|| "?".to_string());
    tracing::info!(%client, "arrêt demandé depuis l'interface web");
    state.quit.notify_one();
    StatusCode::ACCEPTED.into_response()
}
