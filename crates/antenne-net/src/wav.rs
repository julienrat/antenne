//! Flux WAV PCM16 continu sur HTTP — transport de *debug* (latence élevée,
//! dépend du tampon du lecteur). Écoute : `ffplay -fflags nobuffer
//! http://localhost:8080/stream/carte1` ou VLC.

use std::convert::Infallible;

use antenne_codec::Encoder;
use antenne_codec::pcm::Pcm16Encoder;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures_util::{StreamExt, stream};
use tokio::sync::broadcast::{Receiver, error::RecvError};

use antenne_core::AudioFrame;

use crate::{AppState, ListenerGuard};

/// En-tête WAV « streaming » : tailles à 0xFFFFFFFF (longueur inconnue).
pub fn wav_header(sample_rate: u32, channels: u16) -> [u8; 44] {
    let bits: u16 = 16;
    let block_align = channels * (bits / 8);
    let byte_rate = sample_rate * u32::from(block_align);

    let mut h = [0_u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16_u32.to_le_bytes());
    h[20..22].copy_from_slice(&1_u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&bits.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    h
}

struct Listener {
    rx: Receiver<AudioFrame>,
    _guard: ListenerGuard,
    encoder: Pcm16Encoder,
    buf: Vec<u8>,
}

/// `GET /stream/{id}`
pub async fn stream_wav(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Some(card) = state.card(&id) else {
        return (StatusCode::NOT_FOUND, format!("carte « {id} » inconnue")).into_response();
    };
    let Some(guard) = ListenerGuard::acquire(&card.listeners, card.config.max_listeners) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "nombre maximal d'auditeurs atteint",
        )
            .into_response();
    };
    tracing::info!(card = %id, "auditeur WAV connecté");

    let header_bytes =
        Bytes::copy_from_slice(&wav_header(state.sample_rate, card.config.channels.count()));
    let listener = Listener {
        rx: card.frames.subscribe(),
        _guard: guard,
        encoder: Pcm16Encoder::new(state.sample_rate, false),
        buf: Vec::new(),
    };

    let head = stream::once(async move { Ok::<Bytes, Infallible>(header_bytes) });
    let frames = stream::unfold(listener, |mut l| async move {
        loop {
            match l.rx.recv().await {
                Ok(frame) => {
                    l.encoder.encode(&frame.samples, &mut l.buf).ok()?;
                    let chunk = Bytes::copy_from_slice(&l.buf);
                    return Some((Ok::<Bytes, Infallible>(chunk), l));
                }
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("auditeur WAV en retard : {n} trames sautées");
                }
                Err(RecvError::Closed) => return None,
            }
        }
    });

    (
        [
            (header::CONTENT_TYPE, "audio/wav"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(head.chain(frames)),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_well_formed() {
        let h = wav_header(48_000, 2);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(&h[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([h[22], h[23]]), 2);
        assert_eq!(u32::from_le_bytes([h[24], h[25], h[26], h[27]]), 48_000);
        assert_eq!(u32::from_le_bytes([h[28], h[29], h[30], h[31]]), 192_000);
        assert_eq!(u16::from_le_bytes([h[32], h[33]]), 4);
    }
}
