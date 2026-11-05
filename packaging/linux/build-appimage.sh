#!/usr/bin/env bash
# Construit dist/Antenne-<version>-<arch>.AppImage.
#
# Usage : packaging/linux/build-appimage.sh [--appdir-only]
#
# Prérequis : Rust, cmake, clang, libpipewire-0.3-dev, libasound2-dev.
# libopus est compilé et lié statiquement (rien à embarquer). libpipewire et
# libasound viennent du système hôte : elles doivent correspondre à son
# serveur audio. La glibc minimale est celle de la machine de construction ;
# PipeWire ≥ 1.0 est nécessaire pour compiler (libspa 0.10), d'où Ubuntu 24.04.
set -euo pipefail

# CMake 4 refuse le `cmake_minimum_required` (< 3.5) des sources d'Opus
# embarquées par audiopus_sys.
export CMAKE_POLICY_VERSION_MINIMUM="${CMAKE_POLICY_VERSION_MINIMUM:-3.5}"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

ARCH="$(uname -m)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
OUT="dist/Antenne-${VERSION}-${ARCH}.AppImage"

echo "==> Compilation (release, opus statique)"
# audiopus_sys ne se recompile pas quand ces variables changent : on force.
cargo clean --release -p audiopus_sys
LIBOPUS_STATIC=1 LIBOPUS_NO_PKG=1 \
    cargo build --release --locked -p antenne --features opus

APPDIR="target/appimage/AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/antenne" \
    "$APPDIR/usr/share/applications" \
    "$APPDIR/usr/share/icons/hicolor/scalable/apps" \
    "$APPDIR/usr/share/icons/hicolor/256x256/apps"

install -m 755 target/release/antenne "$APPDIR/usr/bin/antenne"
install -m 644 config/antenne.example.toml "$APPDIR/usr/share/antenne/"
install -m 755 packaging/linux/AppRun "$APPDIR/AppRun"
install -m 644 packaging/linux/antenne.desktop "$APPDIR/antenne.desktop"
install -m 644 packaging/linux/antenne.desktop "$APPDIR/usr/share/applications/"
install -m 644 packaging/antenne.svg "$APPDIR/antenne.svg"
install -m 644 packaging/antenne.svg "$APPDIR/usr/share/icons/hicolor/scalable/apps/"
if command -v convert >/dev/null 2>&1; then
    convert packaging/antenne-1024.png -resize 256x256 \
        "$APPDIR/usr/share/icons/hicolor/256x256/apps/antenne.png"
fi
ln -sf antenne.svg "$APPDIR/.DirIcon"

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$APPDIR/antenne.desktop"
fi

# Le binaire ne doit dépendre que de bibliothèques présentes sur tout bureau Linux.
echo "==> Bibliothèques dynamiques"
ldd "$APPDIR/usr/bin/antenne" | sed 's/^/    /'
if ldd "$APPDIR/usr/bin/antenne" | grep -q libopus; then
    echo "erreur : libopus est liée dynamiquement (LIBOPUS_STATIC ignoré ?)" >&2
    exit 1
fi

if [ "${1:-}" = "--appdir-only" ]; then
    echo "==> AppDir prêt (sans assemblage) : $APPDIR"
    exit 0
fi

# appimagetool : celui de $APPIMAGETOOL s'il est fourni, sinon téléchargé.
TOOL="${APPIMAGETOOL:-target/tools/appimagetool-${ARCH}.AppImage}"
if [ ! -x "$TOOL" ]; then
    echo "==> Téléchargement d'appimagetool"
    mkdir -p target/tools
    curl -fL --retry 3 -o "$TOOL" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${ARCH}.AppImage"
    chmod +x "$TOOL"
fi

echo "==> Assemblage de $OUT"
mkdir -p dist
ARCH="$ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream "$APPDIR" "$OUT"
echo "==> Terminé : $OUT"
