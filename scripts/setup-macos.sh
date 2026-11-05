#!/usr/bin/env bash
# Prépare un Mac pour développer Antenne.
set -euo pipefail

echo "==> Outils de compilation Apple"
xcode-select -p >/dev/null 2>&1 || xcode-select --install

echo "==> Homebrew"
if ! command -v brew >/dev/null 2>&1; then
  echo "Installer Homebrew d'abord : https://brew.sh" >&2
  exit 1
fi
brew install pkg-config opus cmake ffmpeg

echo "==> Pilote de bouclage BlackHole 16 canaux (cartes « virtual »)"
brew install --cask blackhole-16ch || echo "Installer BlackHole manuellement : https://existential.audio/blackhole/"

echo "==> Rust"
if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

echo "==> Terminé."
echo "    Au premier lancement, macOS demandera l'autorisation d'accès au micro"
echo "    pour le terminal / VSCode : l'accepter, sinon la capture reste muette."
