# 03 — Plateformes

## Synthèse

| | Linux | macOS | Windows |
|---|---|---|---|
| Hôte audio (cpal) | PipeWire natif (feature `pipewire`) | CoreAudio | WASAPI |
| Cartes « virtual » | **null sink PipeWire** créé par Antenne | canaux d'un pilote **BlackHole** | canaux d'un pilote **VB-CABLE / VAC** |
| Création automatique | ✅ oui | ❌ installer le pilote une fois | ❌ installer le pilote une fois |
| Compilation | gcc/clang, pkg-config, libasound2-dev, libpipewire-0.3-dev | Xcode CLT | MSVC Build Tools |
| Opus (feature) | libopus-dev | `brew install opus` | cmake (libopus compilée) |
| Script | `scripts/setup-linux.sh` | `scripts/setup-macos.sh` | `scripts/setup-windows.ps1` |

## Linux (prioritaire)

**Mécanisme** : pour chaque carte `kind = "virtual"`, Antenne exécute

```sh
pactl load-module module-null-sink sink_name=antenne_<id> \
  sink_properties=device.description="<name>" channels=<1|2> channel_map=<…> rate=48000
```

puis capture le moniteur de ce sink via l'hôte PipeWire de cpal (qui expose les sinks comme
entrées capturables). À l'arrêt, `pactl unload-module <index>`. Si le sink existe déjà (lancement
précédent interrompu), il est réutilisé.

- Fonctionne avec **pipewire-pulse** (défaut sur Ubuntu ≥ 22.10, Linux Mint 22, Fedora…).
- Vérifier : `pactl info | grep "Server Name"` doit mentionner PipeWire.
- Voir les cartes : `pactl list short sinks`, ou `qpwgraph` / `helvum` pour le routage.
- Latence : `PIPEWIRE_LATENCY=256/48000` (≈ 5 ms) peut être passé à l'environnement.

**Évolution (jalon 5)** : créer les nœuds directement via libpipewire (crate `pipewire`) pour
supprimer la dépendance à `pactl` et fixer `node.latency`.

**Dépendances de compilation** : `build-essential pkg-config clang libclang-dev
libasound2-dev libpipewire-0.3-dev` (+ `libopus-dev` pour Opus). ALSA reste requis par cpal
même quand PipeWire est utilisé.

## macOS

**Contrainte** : macOS ne permet pas à une application de créer un périphérique audio à la volée ;
il faut un plug-in de pilote **AudioServerPlugIn** installé dans
`/Library/Audio/Plug-Ins/HAL/` (signé, et notarisé pour la distribution).

**v1** : backend `loopback` sur **BlackHole 16ch** (`brew install --cask blackhole-16ch`).
Les cartes sont réparties dans l'ordre du fichier : carte 1 stéréo → canaux 1-2, carte 2 mono →
canal 3, etc. Les applications choisissent leurs canaux de sortie (routage dans le DAW, ou
« Configuration audio et MIDI » pour créer un périphérique agrégé).

**Permissions** : la capture déclenche la demande d'accès au **micro** (TCC) pour le terminal /
VSCode / l'app. Sans accord, cpal reçoit du silence.

**Licence** : BlackHole est sous **GPL-3.0**. Antenne ne le lie pas (simple périphérique système),
mais le redistribuer dans un paquet Antenne imposerait d'en respecter la licence. Plus simple :
le faire installer séparément.

**Évolution possible** : plug-in maison (C++ avec une bibliothèque d'aide type libASPL, ou Rust
via bindings CoreAudio) qui expose de vrais périphériques « Antenne Carte N ».

## Windows

**Contrainte** : un périphérique audio virtuel = un **pilote noyau** signé par Microsoft
(certificat EV + soumission). Hors périmètre v1.

**v1** : backend `loopback` sur un pilote existant :

- **VB-CABLE** (gratuit, donationware) : 1 câble stéréo → 1 carte stéréo ou 2 mono. Les variantes
  A+B / C+D ajoutent des câbles.
- **Virtual Audio Cable** (payant) : nombre de câbles et de canaux configurable.

Renseigner `platform.loopback_device` avec un extrait du nom vu par `antenne devices`
(ex. `"CABLE Output"`). Pour plusieurs câbles distincts, utiliser plutôt
`source = { kind = "device", device = "…" }` carte par carte.

**Latence** : WASAPI en mode partagé ajoute typiquement ~10 ms ; le mode exclusif (plus bas)
pourra être étudié au jalon 5.

**Débogage** : la configuration *Windows (MSVC)* de `launch.json` utilise le débogueur Visual
Studio (extension `ms-vscode.cpptools`) ; CodeLLDB fonctionne aussi.

## Distribution (plus tard, jalon 7)

- Linux : `.deb` (cargo-deb) et/ou AppImage ; service systemd utilisateur optionnel.
- macOS : `.app` signée + notarisée, ou binaire + Homebrew tap.
- Windows : `.msi` (cargo-wix) ou installeur ; signature de code recommandée.
- Outil transverse envisageable : `cargo-dist` (binaires + installeurs depuis la CI).

## Paquets de distribution

| OS | Paquet | Construction | Dépendances à l'exécution |
|----|--------|--------------|---------------------------|
| Linux x86_64 | AppImage | `packaging/linux/build-appimage.sh` (télécharge `appimagetool`) | glibc ≥ celle de la machine de build (2.39 en local, 2.35 via la CI Ubuntu 22.04), `libpipewire-0.3`, `libasound2`, `pactl` (pipewire-pulse ou pulseaudio-utils) |
| macOS 11+ (arm64 + x86_64) | DMG universel | `packaging/macos/build-dmg.sh`, **sur un Mac uniquement** (SDK CoreAudio, `lipo`, `codesign`, `hdiutil`) ou CI `release.yml` | BlackHole (ou autre pilote de bouclage) pour les cartes virtuelles ; autorisation micro accordée au Terminal |

Non signé par un Developer ID ni notarisé : Gatekeeper demande clic droit > Ouvrir au
premier lancement. La compilation croisée Linux → macOS n'est pas possible
simplement (bindgen de cpal a besoin des en-têtes du SDK Apple).

### Compiler sous Windows

str0m (WebRTC) tire `aws-lc-sys` via dimpl (génération du certificat DTLS), même avec le backend
rust-crypto. Sous Windows, il faut soit **NASM** dans le `PATH`, soit
`AWS_LC_SYS_PREBUILT_NASM=1` (objets pré-assemblés, utilisé par la CI). Opus est compilé depuis
les sources avec **CMake** s'il n'est pas trouvé par pkg-config.
