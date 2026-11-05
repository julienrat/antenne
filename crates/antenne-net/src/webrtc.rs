//! Diffusion WebRTC avec signalisation façon WHEP (draft IETF `wish-whep`,
//! symétrique de WHIP / RFC 9725).
//!
//! ```text
//! Client (ESP32, navigateur)              Antenne
//!   | POST <path>  (Content-Type: application/sdp)
//!   |   offre SDP recvonly
//!   |------------------------------------->|
//!   |                                      | session str0m + socket UDP,
//!   |                                      | seul le codec de la carte est proposé
//!   | 201 Created + Location: <path>/<id>  |
//!   |   réponse SDP sendonly               |
//!   |<-------------------------------------|
//!   |  ICE (UDP) + DTLS + SRTP ...         |
//!   |<===== RTP audio (une trame / paquet) |
//!   | DELETE <path>/<id>  (fin)            |
//! ```
//!
//! Chaque session est une tâche tokio qui possède son `Rtc` (str0m, sans-IO),
//! son socket UDP, son encodeur et un abonnement au `broadcast` de la carte.
//! Elle s'arrête sur `DELETE`, déconnexion ICE, absence de connexion au bout
//! de [`CONNECT_TIMEOUT`], ou fin du flux de la carte (carte modifiée, arrêt).

use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use antenne_codec::Encoder;
use antenne_core::{AudioFrame, Encoding};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use str0m::change::SdpOffer;
use str0m::config::CryptoProvider;
use str0m::format::Codec;
use str0m::media::{Frequency, MediaKind, MediaTime, Mid, Pt};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcConfig};
use tokio::net::UdpSocket;
use tokio::sync::Notify;
use tokio::sync::broadcast::error::RecvError;

use crate::{AppState, CardChannel, CardStatus, ListenerGuard};

/// Délai pour établir ICE + DTLS après la réponse SDP.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Sessions en cours, par identifiant (dernier segment de `Location`).
pub type Sessions = Arc<Mutex<HashMap<String, Session>>>;

#[derive(Debug)]
pub struct Session {
    pub card_id: String,
    stop: Arc<Notify>,
}

fn lock(sessions: &Sessions) -> std::sync::MutexGuard<'_, HashMap<String, Session>> {
    sessions
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn crypto() -> Arc<CryptoProvider> {
    static PROVIDER: OnceLock<Arc<CryptoProvider>> = OnceLock::new();
    PROVIDER
        .get_or_init(|| Arc::new(str0m::crypto::from_feature_flags()))
        .clone()
}

fn plain(status: StatusCode, message: impl Into<String>) -> Response {
    (status, message.into()).into_response()
}

/// Codec WebRTC correspondant à l'encodage de la carte.
fn webrtc_codec(encoding: &Encoding) -> Option<(Codec, &'static str)> {
    match encoding {
        Encoding::Opus { .. } => Some((Codec::Opus, "opus/48000")),
        Encoding::G711u => Some((Codec::PCMU, "pcmu/8000")),
        Encoding::G711a => Some((Codec::PCMA, "pcma/8000")),
        Encoding::Pcm16 => None,
    }
}

/// Adresse locale par laquelle on joint `peer` (aucun paquet n'est envoyé).
fn local_ip_towards(peer: IpAddr) -> Option<IpAddr> {
    let any: SocketAddr = if peer.is_ipv4() {
        (Ipv4Addr::UNSPECIFIED, 0).into()
    } else {
        (Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let socket = std::net::UdpSocket::bind(any).ok()?;
    socket.connect((peer, 9)).ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

/// Adresse à annoncer au client dans le candidat ICE.
///
/// Pour un client local, on préfère l'adresse du réseau local à 127.0.0.1
/// (que Firefox refuse dans les candidats).
fn candidate_ip(public_ip: Option<IpAddr>, peer: Option<IpAddr>) -> IpAddr {
    if let Some(ip) = public_ip {
        return ip;
    }
    match peer {
        Some(peer) if peer.is_loopback() => {
            let probe: IpAddr = if peer.is_ipv4() {
                Ipv4Addr::new(192, 0, 2, 1).into()
            } else {
                "2001:db8::1".parse().unwrap_or(peer)
            };
            local_ip_towards(probe).unwrap_or(peer)
        }
        Some(peer) => local_ip_towards(peer).unwrap_or(peer),
        None => Ipv4Addr::LOCALHOST.into(),
    }
}

fn new_session_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let hash = RandomState::new().hash_one((n, Instant::now()));
    format!("{hash:016x}")
}

/// `POST <path>` : offre SDP d'un client.
pub async fn whep_offer(
    state: &AppState,
    card: Arc<CardChannel>,
    headers: &HeaderMap,
    body: String,
    peer: Option<SocketAddr>,
) -> Response {
    let is_sdp = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/sdp"));
    if !is_sdp {
        return plain(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Content-Type attendu : application/sdp",
        );
    }
    if body.trim().is_empty() {
        return plain(StatusCode::BAD_REQUEST, "offre SDP vide");
    }
    if !matches!(card.status, CardStatus::Running { .. }) {
        return plain(StatusCode::SERVICE_UNAVAILABLE, "carte hors service");
    }
    let Some((codec, rtpmap)) = webrtc_codec(&card.config.encoding) else {
        return plain(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "PCM 16 bits n'existe pas en WebRTC : choisir Opus ou G.711 pour cette carte",
        );
    };
    let encoder = match antenne_codec::make_encoder(
        &card.config.encoding,
        card.config.channels,
        state.sample_rate,
    ) {
        Ok(encoder) => encoder,
        Err(e) => return plain(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let offer = match SdpOffer::from_sdp_string(&body) {
        Ok(offer) => offer,
        Err(e) => return plain(StatusCode::BAD_REQUEST, format!("offre SDP invalide : {e}")),
    };
    let Some(guard) = ListenerGuard::acquire(&card.listeners, card.config.max_listeners) else {
        return plain(
            StatusCode::SERVICE_UNAVAILABLE,
            "nombre maximal d'auditeurs atteint",
        );
    };

    let ip = candidate_ip(state.public_ip, peer.map(|p| p.ip()));
    let socket = match UdpSocket::bind((ip, 0)).await {
        Ok(socket) => socket,
        Err(e) => {
            return plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("socket UDP sur {ip} : {e}"),
            );
        }
    };
    let local = match socket.local_addr() {
        Ok(addr) => addr,
        Err(e) => return plain(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let config = RtcConfig::new()
        .set_crypto_provider(crypto())
        .clear_codecs();
    let config = match codec {
        Codec::Opus => config.enable_opus(true, false),
        Codec::PCMU => config.enable_pcmu(true, false),
        _ => config.enable_pcma(true, false),
    };
    let mut rtc = config.build(Instant::now());
    match Candidate::host(local, "udp") {
        Ok(candidate) => {
            rtc.add_local_candidate(candidate);
        }
        Err(e) => {
            return plain(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("candidat ICE : {e}"),
            );
        }
    }
    let answer = match rtc.sdp_api().accept_offer(offer) {
        Ok(answer) => answer,
        Err(e) => return plain(StatusCode::BAD_REQUEST, format!("offre refusée : {e}")),
    };
    let answer = answer.to_sdp_string();
    if !answer.to_ascii_lowercase().contains(rtpmap) {
        return plain(
            StatusCode::NOT_ACCEPTABLE,
            format!(
                "le client ne propose pas le codec de la carte ({})",
                card.config.encoding.label()
            ),
        );
    }

    let id = new_session_id();
    let stop = Arc::new(Notify::new());
    lock(&state.sessions).insert(
        id.clone(),
        Session {
            card_id: card.config.id.clone(),
            stop: Arc::clone(&stop),
        },
    );
    tracing::info!(
        card = %card.config.id, session = %id, client = ?peer, udp = %local,
        codec = card.config.encoding.label(), "session WebRTC ouverte"
    );

    let session = SessionTask {
        id: id.clone(),
        card_id: card.config.id.clone(),
        rtc,
        socket,
        local,
        codec,
        encoder,
        frames: card.frames.subscribe(),
        stop,
        _guard: guard,
    };
    let sessions = Arc::clone(&state.sessions);
    tokio::spawn(async move {
        let card_id = session.card_id.clone();
        let id = session.id.clone();
        match session.run().await {
            Ok(reason) => {
                tracing::info!(card = %card_id, session = %id, "session WebRTC fermée : {reason}")
            }
            Err(e) => {
                tracing::warn!(card = %card_id, session = %id, "session WebRTC interrompue : {e}")
            }
        }
        lock(&sessions).remove(&id);
    });

    let location = format!("{}/{}", card.config.whep_path(), id);
    let mut response = (StatusCode::CREATED, answer).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/sdp"),
    );
    if let Ok(value) = HeaderValue::from_str(&location) {
        headers.insert(header::LOCATION, value);
    }
    response
}

/// `DELETE <path>/<id>` : fin de session demandée par le client.
pub fn whep_delete(state: &AppState, card_id: &str, session_id: &str) -> Response {
    let removed = {
        let mut sessions = lock(&state.sessions);
        match sessions.get(session_id) {
            Some(s) if s.card_id == card_id => sessions.remove(session_id),
            _ => None,
        }
    };
    match removed {
        Some(session) => {
            session.stop.notify_one();
            StatusCode::OK.into_response()
        }
        None => plain(StatusCode::NOT_FOUND, "session inconnue"),
    }
}

struct SessionTask {
    id: String,
    card_id: String,
    rtc: Rtc,
    socket: UdpSocket,
    local: SocketAddr,
    codec: Codec,
    encoder: Box<dyn Encoder>,
    frames: tokio::sync::broadcast::Receiver<AudioFrame>,
    stop: Arc<Notify>,
    _guard: ListenerGuard,
}

type SessionResult = Result<&'static str, Box<dyn std::error::Error + Send + Sync>>;

impl SessionTask {
    async fn run(mut self) -> SessionResult {
        let started = Instant::now();
        let clock = self.encoder.output_sample_rate();
        let frequency = Frequency::new(clock).ok_or("horloge RTP nulle")?;
        let mut media: Option<(Mid, Pt)> = None;
        let mut connected = false;
        let mut rtp_time: u64 = 0;
        let mut payload = Vec::new();
        let mut buf = vec![0u8; 2000];

        loop {
            // 1. Vider les sorties de str0m jusqu'à la prochaine échéance.
            let deadline = loop {
                match self.rtc.poll_output()? {
                    Output::Timeout(at) => break at,
                    Output::Transmit(t) => {
                        self.socket.send_to(&t.contents, t.destination).await?;
                    }
                    Output::Event(event) => match event {
                        Event::MediaAdded(m) if m.kind == MediaKind::Audio => {
                            let pt = self.rtc.writer(m.mid).and_then(|w| {
                                w.payload_params()
                                    .find(|p| p.spec().codec == self.codec)
                                    .map(|p| p.pt())
                            });
                            match pt {
                                Some(pt) => media = Some((m.mid, pt)),
                                None => return Ok("codec absent de la négociation"),
                            }
                        }
                        Event::Connected => {
                            connected = true;
                            tracing::info!(
                                card = %self.card_id, session = %self.id,
                                "WebRTC connecté en {} ms",
                                started.elapsed().as_millis()
                            );
                        }
                        Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                            return Ok("client déconnecté (ICE)");
                        }
                        _ => {}
                    },
                }
            };
            if !self.rtc.is_alive() {
                return Ok("session terminée");
            }
            if !connected && started.elapsed() > CONNECT_TIMEOUT {
                return Ok("aucune connexion ICE / DTLS dans les délais");
            }

            // 2. Attendre : paquet UDP, échéance str0m, trame audio ou arrêt.
            let sleep = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline));
            tokio::select! {
                received = self.socket.recv_from(&mut buf) => {
                    let (n, source) = received?;
                    let Ok(contents) = buf[..n].try_into() else { continue };
                    let input = Input::Receive(
                        Instant::now(),
                        Receive { proto: Protocol::Udp, source, destination: self.local, contents },
                    );
                    if self.rtc.accepts(&input) {
                        self.rtc.handle_input(input)?;
                    }
                }
                () = sleep => {
                    self.rtc.handle_input(Input::Timeout(Instant::now()))?;
                }
                frame = self.frames.recv() => match frame {
                    Ok(frame) => {
                        let step = frame.frames_per_channel() as u64 * u64::from(clock)
                            / u64::from(frame.sample_rate.max(1));
                        if let (true, Some((mid, pt))) = (connected, media) {
                            self.encoder.encode(&frame.samples, &mut payload)?;
                            if let Some(writer) = self.rtc.writer(mid) {
                                writer.write(
                                    pt,
                                    Instant::now(),
                                    MediaTime::new(rtp_time, frequency),
                                    payload.as_slice(),
                                )?;
                            }
                        }
                        rtp_time += step;
                    }
                    Err(RecvError::Lagged(n)) => {
                        tracing::debug!(session = %self.id, "{n} trames sautées");
                    }
                    Err(RecvError::Closed) => {
                        self.rtc.disconnect();
                        return Ok("flux de la carte terminé");
                    }
                },
                () = self.stop.notified() => {
                    self.rtc.disconnect();
                    return Ok("fermée par le client (DELETE)");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codecs_map_to_webrtc() {
        assert_eq!(
            webrtc_codec(&Encoding::G711u).map(|c| c.0),
            Some(Codec::PCMU)
        );
        assert_eq!(
            webrtc_codec(&Encoding::G711a).map(|c| c.0),
            Some(Codec::PCMA)
        );
        assert_eq!(
            webrtc_codec(&Encoding::default()).map(|c| c.0),
            Some(Codec::Opus)
        );
        assert!(webrtc_codec(&Encoding::Pcm16).is_none());
    }

    #[test]
    fn candidate_ip_prefers_public_then_route() {
        let public: IpAddr = "10.1.2.3".parse().unwrap();
        assert_eq!(candidate_ip(Some(public), None), public);
        // Vers soi-même, l'interface utilisée est la boucle locale.
        assert_eq!(
            local_ip_towards(Ipv4Addr::LOCALHOST.into()),
            Some(Ipv4Addr::LOCALHOST.into())
        );
        assert_eq!(candidate_ip(None, None), IpAddr::from(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn session_ids_are_unique_hex() {
        let a = new_session_id();
        let b = new_session_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
