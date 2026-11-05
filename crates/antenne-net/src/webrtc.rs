//! Signalisation WebRTC façon WHEP (draft IETF `wish-whep`, symétrique de
//! WHIP / RFC 9725).
//!
//! Échange visé (jalon 3) :
//!
//! ```text
//! ESP32                                   Antenne
//!   | POST <path>  (Content-Type: application/sdp)
//!   |   offre SDP recvonly, audio Opus ou PCMU/PCMA
//!   |------------------------------------->|
//!   |                                      | crée une session str0m,
//!   |                                      | choisit le codec de la carte
//!   | 201 Created + Location: <path>/<sid> |
//!   |   réponse SDP sendonly               |
//!   |<-------------------------------------|
//!   |  ICE (UDP) + DTLS + SRTP ...         |
//!   |<===== RTP audio (trames 10 ms) ======|
//!   | DELETE <path>/<sid>  (fin)           |
//! ```
//!
//! Implémentation prévue : crate `str0m` (WebRTC « sans-IO », piloté par une
//! boucle tokio + un socket UDP partagé). Chaque session s'abonne au
//! `broadcast` de sa carte, encode (ou réutilise l'encodage partagé) et
//! écrit les paquets via `str0m::media::Writer`.

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

use std::sync::Arc;

use crate::CardChannel;

/// `POST <path>` : offre SDP d'un client.
pub async fn whep_offer(card: Arc<CardChannel>, headers: HeaderMap, body: String) -> Response {
    let is_sdp = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/sdp"));
    if !is_sdp {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Content-Type attendu : application/sdp",
        )
            .into_response();
    }
    if body.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "offre SDP vide").into_response();
    }

    tracing::info!(
        card = %card.config.id,
        codec = card.config.encoding.label(),
        offer_bytes = body.len(),
        "offre WHEP reçue (non traitée : jalon 3)"
    );

    // TODO(jalon 3) : négociation str0m, réponse 201 + SDP + en-tête Location.
    (
        StatusCode::NOT_IMPLEMENTED,
        "WebRTC pas encore implémenté (jalon 3) — utiliser /stream/<id> pour tester",
    )
        .into_response()
}
