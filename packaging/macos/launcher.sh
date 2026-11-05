#!/bin/bash
# Lanceur de Antenne.app (Contents/MacOS/Antenne).
#
# Antenne est un serveur sans fenêtre : on l'ouvre dans le Terminal (journaux
# visibles, Ctrl+C pour arrêter) avec la configuration de l'utilisateur,
# créée depuis l'exemple au premier lancement, puis on ouvre l'interface web.

HERE="$(cd "$(dirname "$0")" && pwd)"
RES="$HERE/../Resources"
BIN="$HERE/antenne-server"

CONF_DIR="$HOME/Library/Application Support/Antenne"
CONF="$CONF_DIR/antenne.toml"
if [ ! -f "$CONF" ]; then
    mkdir -p "$CONF_DIR"
    cp "$RES/antenne.example.toml" "$CONF"
    chmod u+w "$CONF"
fi

PORT="$(sed -n 's/^[[:space:]]*bind[[:space:]]*=[[:space:]]*"[^"]*:\([0-9][0-9]*\)".*/\1/p' "$CONF" | head -n 1)"
URL="http://localhost:${PORT:-8080}/"

# Script exécuté dans le Terminal (chemins entre apostrophes : ils contiennent des espaces).
quote() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }
CMD="clear; echo 'Antenne — interface : $URL  (Ctrl+C pour arrêter)'; echo 'Configuration : '$(quote "$CONF"); $(quote "$BIN") --config $(quote "$CONF") run; echo; echo 'Antenne est arrêté. Vous pouvez fermer cette fenêtre.'"

osascript - "$CMD" <<'APPLESCRIPT'
on run argv
    tell application "Terminal"
        activate
        do script (item 1 of argv)
    end tell
end run
APPLESCRIPT

(sleep 3 && open "$URL") &
