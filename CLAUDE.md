# CLAUDE.md — contexte pour Claude Code

## Le projet en bref

**Antenne** (In Corpo) : logiciel Rust tout-en-un qui crée **plusieurs cartes son virtuelles**
et diffuse **chaque carte en unicast faible latence** vers des **ESP32** (un flux par ESP32).
Par carte : chemin WebRTC (WHEP), mono/stéréo, encodage. Cible : Linux d'abord, puis macOS et
Windows.

Lire avant toute tâche importante :

- `docs/01-cahier-des-charges.md` — besoins, exigences, questions ouvertes
- `docs/02-architecture.md` — crates, threads, décisions (tableau D1…)
- `docs/03-plateformes.md` — cartes virtuelles et dépendances par OS
- `docs/04-esp32.md` — protocole côté client
- `docs/05-roadmap.md` — jalons ; **travailler jalon par jalon**

## Commandes

```sh
cargo build                     # binaire `antenne` (membre par défaut)
cargo test --workspace          # tous les tests
cargo lint                      # alias : clippy --workspace --all-targets -D warnings
cargo fmt --all
cargo dev                       # alias : run avec config/antenne.example.toml
cargo devices                   # alias : liste les entrées audio
cargo run -p antenne --features opus -- --config config/antenne.toml run
cargo test -p antenne-codec --features opus
packaging/linux/build-appimage.sh   # dist/Antenne-<v>-x86_64.AppImage
packaging/macos/build-dmg.sh        # sur un Mac : dist/Antenne-<v>-macos-universal.dmg
```

Écouter un flux de debug : `ffplay -nodisp -fflags nobuffer http://localhost:8080/stream/<id>`.
Logs : `RUST_LOG=info,antenne=debug`.

## Carte du code

```
crates/antenne-core   config.rs (TOML + validate), frame.rs (AudioFrame)
crates/antenne-audio  capture.rs (cpal), tone.rs, framer.rs, devices.rs,
                      virtual_device/{mod,pipewire,loopback}.rs
crates/antenne-codec  lib.rs (trait Encoder, make_encoder), pcm.rs, g711.rs, opus_enc.rs
crates/antenne-net    lib.rs (router, AppState, serve), wav.rs, webrtc.rs (stub WHEP),
                      api.rs (CRUD /api/cards), ui.rs + ui/index.html (interface web sur `/`),
                      meter.rs (vu-mètres)
crates/antenne        main.rs, cli.rs, app.rs (orchestration), cards.rs (CardManager : cartes à chaud)
config/antenne.example.toml   exemple, testé par antenne-core (doit rester valide)
```

## Règles

**Temps réel (non négociable)** — dans le callback cpal (`capture.rs::build`) : aucune
allocation, aucun verrou (`Mutex`, `RwLock`), aucun log, aucun appel système, aucun `await`.
Seulement copie vers le ring buffer `rtrb` et compteurs atomiques.

**Multiplateforme**
- Code spécifique à un OS derrière `#[cfg(target_os = "…")]`, regroupé dans un module dédié
  (modèle : `virtual_device/`). Fournir une alternative (ou une erreur claire) sur les autres OS.
- Les dépendances spécifiques à un OS vont dans `[target.'cfg(…)'.dependencies]`.
- Ne jamais supposer qu'une capacité Linux existe ailleurs ; vérifier dans
  `docs/03-plateformes.md` et le mettre à jour.
- Je ne peux tester que l'OS local : s'appuyer sur la CI (`.github/workflows/ci.yml`) pour
  macOS / Windows et le dire quand un changement n'a pas pu être testé localement.

**Style**
- Identifiants de code en anglais ; commentaires, docs, messages d'erreur et logs en français.
- Pas d'`unwrap()` / `expect()` hors tests ; `anyhow::Context` pour contextualiser.
- `antenne-core` reste sans dépendance audio / réseau.
- Toute nouvelle clé de configuration : struct dans `config.rs` + validation + test +
  exemple commenté dans `config/antenne.example.toml`.
- Nouvelle décision d'architecture → ligne dans le tableau de `docs/02-architecture.md`.
- Tâche terminée → cocher dans `docs/05-roadmap.md`.

**Avant de rendre la main** : `cargo fmt --all`, `cargo lint`, `cargo test --workspace`.

## Pièges connus

- Le squelette a été écrit sans compilateur ni accès à crates.io : au premier build, des
  écarts d'API sont possibles (surtout cpal 0.18 et la feature `opus`) — les corriger d'abord.
- cpal 0.18 : les flux démarrent en pause (`stream.play()`), `StreamConfig` passé par valeur,
  erreurs unifiées dans `cpal::Error`.
- `cpal::Stream` n'est pas `Send` partout : le créer et le garder dans son thread.
- Linux : la feature `pipewire` de cpal exige `libpipewire-0.3-dev` et `clang` (bindgen).
- G.711 = mono 8 kHz uniquement (validé dans la config).
- Les chemins `/health`, `/api/*`, `/stream/*` sont réservés (validation de `path`).
