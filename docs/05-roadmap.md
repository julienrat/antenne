# 05 — Roadmap

Chaque jalon se termine par : `cargo fmt`, `cargo lint`, `cargo test` verts, CI verte sur les
trois OS, docs à jour. Cocher au fur et à mesure.

## Jalon 0 — Socle (ce dossier) ✅

- [x] Workspace Cargo (core, audio, codec, net, binaire)
- [x] Configuration TOML validée + exemple testé automatiquement
- [x] Générateur de tonalité, capture cpal, backends PipeWire et loopback
- [x] Encodeurs PCM16 et G.711 (testés), Opus derrière la feature `opus`
- [x] Serveur HTTP : `/health`, `/api/cards`, `/stream/<id>`, `POST <path>` (stub WHEP)
- [x] VSCode (tâches, debug), CI Linux / macOS / Windows, CLAUDE.md
- [x] **Premier `cargo build` sur la machine de Julien** et correction des éventuelles erreurs
      de compilation (le squelette a été écrit sans accès à crates.io) — aucun écart d'API cpal

## Jalon 1 — Linux de bout en bout (flux de debug) ✅

- [x] `antenne run` crée les null sinks ; ils apparaissent dans les réglages son
- [x] Jouer de la musique vers « Antenne Carte 1 » → audible via `ffplay …/stream/carte1`
- [x] Vérifier le nom sous lequel cpal (hôte PipeWire) expose le sink, ajuster
      `capture_patterns` si besoin — c'est `device.description` ; pipewire-pulse le coupait au
      premier espace faute de guillemets (corrigé dans `pipewire.rs`)
- [x] Suppression propre des sinks à Ctrl+C, réutilisation si déjà présents
- [x] `antenne devices` affiche les entrées de façon lisible

**Critère** : 3 cartes simultanées pendant 1 h sans xrun ni dérive. *(pas encore mesuré)*

## Jalon 2 — Encodage et mesures

- [ ] Activer Opus par défaut (libopus disponible sur les 3 CI)
- [ ] Rééchantillonnage propre (`rubato`) si le périphérique n'est pas à 48 kHz ; G.711 avec
      vrai filtre anti-repliement
- [ ] Réduire la latence de capture (`BufferSize::Fixed`, réveil du thread pompe)
- [ ] Métriques : trames, overruns, auditeurs, latence capture→envoi (`/api/metrics`)
- [ ] Banc de mesure de latence (clic → enregistrement)

## Jalon 3 — WebRTC (WHEP) avec str0m

- [ ] Session str0m par client, socket UDP partagé, boucle tokio
- [ ] `POST <path>` → `201` + SDP + `Location` ; `DELETE` pour fermer
- [ ] Opus et PCMU/PCMA négociés selon `encoding` de la carte
- [ ] Choix du backend crypto (aws-lc-rs par défaut ; rust-crypto si cmake pose problème)
- [ ] Page HTML de test servie par Antenne (navigateur comme client WHEP)
- [ ] `max_listeners`, expiration des sessions mortes
- [ ] *(3 bis, si besoin)* transport RTP/UDP direct sans chiffrement

## Jalon 4 — Client ESP32 (prototype)

- [ ] Choisir la carte (ESP32-S3 + PSRAM conseillé) et le DAC I2S
- [ ] Signalisation « WHEP » personnalisée dans esp-webrtc-solution (ou RTP direct)
- [ ] Mesurer la latence bout en bout, régler le tampon de gigue
- [ ] Reconnexion automatique (Wi-Fi, redémarrage d'Antenne)

## Jalon 5 — macOS et Windows

- [ ] macOS + BlackHole 16ch : 4 cartes stéréo, permission micro gérée, doc de routage
- [ ] Windows + VB-CABLE / VAC : 1 à N cartes, mode WASAPI exclusif à évaluer
- [ ] Linux : remplacer `pactl` par libpipewire (crate `pipewire`)

## Jalon 6 — Robustesse

- [ ] Branchement / débranchement de périphériques à chaud, relance d'une carte en échec
- [x] Modification des cartes sans redémarrer (API `/api/cards`, voir D11)
- [ ] Rechargement du fichier de configuration modifié à la main, sans redémarrer
- [ ] Journalisation fichier, niveau réglable

## Jalon 7 — Interface et distribution

- [x] Interface web de supervision, en lecture seule (`GET /`) : état des cartes, vu-mètres, auditeurs, écoute du flux de debug
- [x] Interface de configuration : ajout / modification / suppression / mise en service des cartes depuis la page web, appliqué à chaud et enregistré dans le TOML
- [x] AppImage Linux x86_64 (`packaging/linux/build-appimage.sh`) et DMG macOS universel signé ad hoc (`packaging/macos/build-dmg.sh`, sur Mac ou via `.github/workflows/release.yml`)
- [ ] Signature Developer ID + notarisation Apple, `.deb`, `.msi` (voir 03-plateformes.md)
- [ ] Lancement au démarrage de session
