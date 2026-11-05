#!/usr/bin/env bash
# Construit dist/Antenne-<version>-macos-universal.dmg (à lancer sur un Mac).
#
# Prérequis : Xcode Command Line Tools, Rust (rustup), cmake (brew install cmake).
# Le binaire est universel (Apple Silicon + Intel) ; libopus est compilé et
# lié statiquement. La signature est « ad hoc » : sans compte Apple Developer
# (Developer ID + notarisation), Gatekeeper demandera une confirmation au
# premier lancement (clic droit > Ouvrir).
set -euo pipefail

# CMake 4 refuse le `cmake_minimum_required` (< 3.5) des sources d'Opus
# embarquées par audiopus_sys.
export CMAKE_POLICY_VERSION_MINIMUM="${CMAKE_POLICY_VERSION_MINIMUM:-3.5}"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "erreur : ce script ne fonctionne que sur macOS (SDK CoreAudio, hdiutil, codesign)" >&2
    exit 1
fi

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"

echo "==> Compilation (release, opus statique) : ${TARGETS[*]}"
for target in "${TARGETS[@]}"; do
    rustup target add "$target" >/dev/null
    cargo clean --release --target "$target" -p audiopus_sys
    LIBOPUS_STATIC=1 LIBOPUS_NO_PKG=1 \
        cargo build --release --locked -p antenne --features opus --target "$target"
done

WORK="target/macos"
APP="$WORK/Antenne.app"
rm -rf "$WORK"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

lipo -create -output "$APP/Contents/MacOS/antenne-server" \
    "target/aarch64-apple-darwin/release/antenne" \
    "target/x86_64-apple-darwin/release/antenne"
install -m 755 packaging/macos/launcher.sh "$APP/Contents/MacOS/Antenne"
install -m 644 config/antenne.example.toml "$APP/Contents/Resources/"

echo "==> Icône"
ICONSET="$WORK/antenne.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" packaging/antenne-1024.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" packaging/antenne-1024.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/antenne.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Antenne</string>
    <key>CFBundleDisplayName</key><string>Antenne</string>
    <key>CFBundleIdentifier</key><string>fr.incorpo.antenne</string>
    <key>CFBundleVersion</key><string>${VERSION}</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleExecutable</key><string>Antenne</string>
    <key>CFBundleIconFile</key><string>antenne</string>
    <key>LSMinimumSystemVersion</key><string>${MACOSX_DEPLOYMENT_TARGET}</string>
    <key>NSMicrophoneUsageDescription</key>
    <string>Antenne capture les entrées audio (BlackHole, interfaces, micros) pour les diffuser.</string>
    <key>NSAppleEventsUsageDescription</key>
    <string>Antenne ouvre une fenêtre du Terminal pour afficher son journal.</string>
</dict>
</plist>
PLIST

echo "==> Signature ad hoc"
codesign --force --sign - "$APP/Contents/MacOS/antenne-server"
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"
lipo -info "$APP/Contents/MacOS/antenne-server"

echo "==> Image disque"
STAGE="$WORK/dmg"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
cat > "$STAGE/LISEZMOI.txt" <<'TXT'
Antenne — installation sur macOS

1. Glisser Antenne.app dans Applications.
2. Premier lancement : clic droit sur Antenne.app > Ouvrir > Ouvrir
   (l'application n'est pas notarisée par Apple).
3. Les cartes « virtuelles » utilisent un pilote de bouclage : installer
   BlackHole 16ch (8 cartes stéréo) ou 64ch (32 cartes stéréo) depuis
   https://existential.audio/blackhole/ ou `brew install --cask blackhole-16ch`,
   puis vérifier loopback_device dans
   ~/Library/Application Support/Antenne/antenne.toml.
4. Au premier lancement, accepter : « contrôler Terminal », l'accès au micro
   pour le Terminal (sinon les cartes restent silencieuses) et les connexions
   entrantes (sinon les ESP32 ne peuvent pas se connecter).

Guide complet (autorisations, routage du son, dépannage) :
https://github.com/julienrat/antenne/blob/main/docs/07-macos.md

Antenne s'ouvre dans une fenêtre du Terminal (Ctrl+C pour l'arrêter) et
l'interface dans le navigateur : http://localhost:8080/
TXT

mkdir -p dist
OUT="dist/Antenne-${VERSION}-macos-universal.dmg"
rm -f "$OUT"
hdiutil create -volname "Antenne ${VERSION}" -srcfolder "$STAGE" -ov -format UDZO "$OUT"
echo "==> Terminé : $OUT"
