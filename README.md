# Antenne

Cartes son virtuelles + diffusion audio **unicast faible latence** vers des **ESP32**.
Un seul programme Rust, pour **Linux**, **macOS** et **Windows**.

```
Applications ──► Antenne Carte 1 ──┐                    ┌──► ESP32 salon
                 Antenne Carte 2 ──┼──► Antenne ──Wi-Fi─┼──► ESP32 cuisine
                 Antenne Carte 3 ──┘   (capture,        └──► ESP32 atelier
                                        encodage, WebRTC)
```

> **État** : jalon 0 (socle). Capture, cartes virtuelles Linux, G.711/PCM et flux HTTP de
> test sont en place ; WebRTC arrive au jalon 3. Voir [la roadmap](docs/05-roadmap.md).

## Démarrer

### 0. Si un dossier `ide-config/` est présent

Il contient les fichiers cachés (`.vscode/`, `.cargo/`, `.claude/`, `.github/`) livrés à part.
Les mettre en place une fois : `bash scripts/install-ide-config.sh`.

### 1. Préparer la machine

| OS | Commande |
|----|----------|
| Linux (Debian/Ubuntu/Mint) | `bash scripts/setup-linux.sh` |
| macOS | `bash scripts/setup-macos.sh` |
| Windows | `powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1` |

### 2. Ouvrir dans VSCode

```sh
code .
```

Accepter les **extensions recommandées** (rust-analyzer, CodeLLDB, Even Better TOML, Claude Code…).
Sous Windows, ajouter `ms-vscode.cpptools` pour le débogueur Visual Studio.

### 3. Compiler, tester, lancer

```sh
cargo build
cargo test --workspace
cargo dev            # lance avec config/antenne.example.toml
```

Dans VSCode : `Ctrl+Shift+B` (build), **Terminal › Exécuter la tâche** (tests, clippy, run…),
`F5` (débogage).

### 4. Écouter

```sh
curl http://localhost:8080/api/cards
ffplay -nodisp -fflags nobuffer -flags low_delay http://localhost:8080/stream/test
```

Sous Linux, jouer un son vers « Antenne Carte 1 » puis écouter `/stream/carte1`.

## Configuration

Copier `config/antenne.example.toml` en `config/antenne.toml` (ignoré par git) :

```toml
[[card]]
id = "carte1"
name = "Antenne Carte 1"
channels = "stereo"                               # ou "mono"
source = { kind = "virtual" }                     # ou device / tone
encoding = { codec = "opus", bitrate_kbps = 96 }  # ou g711u, g711a, pcm16
path = "/whep/carte1"                             # adresse WebRTC de la carte
```

Valider : `cargo run -p antenne -- --config config/antenne.toml check`.

## Structure

```
.vscode/            réglages, tâches, débogage, extensions
.github/workflows/  CI : fmt, clippy, tests sur Linux / macOS / Windows
.claude/            permissions Claude Code
config/             configuration d'exemple
crates/
  antenne-core/     configuration, types communs
  antenne-audio/    capture cpal, cartes virtuelles (PipeWire, loopback)
  antenne-codec/    PCM16, G.711, Opus (feature)
  antenne-net/      serveur HTTP : API, WHEP, flux de debug
  antenne/          binaire et orchestration
docs/               cahier des charges, architecture, plateformes, ESP32, roadmap
scripts/            installation des dépendances par OS
CLAUDE.md           contexte pour Claude Code
```

## Documentation

1. [Cahier des charges](docs/01-cahier-des-charges.md)
2. [Architecture](docs/02-architecture.md)
3. [Plateformes](docs/03-plateformes.md)
4. [Clients ESP32](docs/04-esp32.md)
5. [Roadmap](docs/05-roadmap.md)

## Licence

À définir.
