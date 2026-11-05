//! Interface web de supervision : la page elle-même et les deux routes JSON
//! qu'elle interroge en plus de `/api/cards`.

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

use axum::Json;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use serde::Serialize;

use crate::{AppState, CardStatus};

/// Page unique, intégrée au binaire : rien à installer à côté d'Antenne.
const INDEX_HTML: &str = include_str!("ui/index.html");

pub(crate) async fn index() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        INDEX_HTML,
    )
}

#[derive(Debug, Serialize)]
pub(crate) struct ServerStatus {
    version: &'static str,
    sample_rate: u32,
    frame_ms: u32,
    virtual_backend: Option<String>,
    uptime_s: u64,
    cards_total: usize,
    cards_running: usize,
    listeners_total: usize,
}

pub(crate) async fn status(State(state): State<AppState>) -> Json<ServerStatus> {
    let cards = state.cards();
    Json(ServerStatus {
        version: env!("CARGO_PKG_VERSION"),
        sample_rate: state.sample_rate,
        frame_ms: state.frame_ms,
        virtual_backend: state.virtual_backend.clone(),
        uptime_s: state.started_at.elapsed().as_secs(),
        cards_total: cards.len(),
        cards_running: cards
            .iter()
            .filter(|c| matches!(c.status, CardStatus::Running { .. }))
            .count(),
        listeners_total: cards
            .iter()
            .map(|c| c.listeners.load(Ordering::Relaxed))
            .sum(),
    })
}

/// `{ "<id carte>": [crête canal 1, crête canal 2?], … }`, valeurs linéaires `0..=1`.
pub(crate) async fn levels(State(state): State<AppState>) -> Json<BTreeMap<String, Vec<f32>>> {
    Json(
        state
            .cards()
            .iter()
            .map(|c| {
                let channels = usize::from(c.config.channels.count());
                (c.config.id.clone(), c.meter.levels(channels))
            })
            .collect(),
    )
}
