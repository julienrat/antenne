#!/usr/bin/env bash
# Prépare une machine Linux (Debian / Ubuntu / Linux Mint) pour développer Antenne.
set -euo pipefail

echo "==> Paquets système (sudo)"
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential pkg-config clang libclang-dev cmake \
  libasound2-dev libpipewire-0.3-dev libopus-dev \
  pulseaudio-utils ffmpeg

echo "==> Rust"
if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi
rustup show active-toolchain || rustup default stable

echo "==> Vérifications"
if pactl info 2>/dev/null | grep -qi pipewire; then
  echo "OK : serveur audio PipeWire (pipewire-pulse) détecté"
else
  echo "ATTENTION : PipeWire non détecté via pactl. Antenne v1 cible PipeWire."
fi

echo "==> Terminé. Ensuite : cargo build && cargo test"
