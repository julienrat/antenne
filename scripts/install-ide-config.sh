#!/usr/bin/env bash
# Installe les fichiers de configuration « cachés » (.vscode, .cargo, .claude, .github)
# livrés dans ide-config/. À lancer une fois depuis la racine du projet :
#   bash scripts/install-ide-config.sh
set -euo pipefail
cd "$(dirname "$0")/.."

src=ide-config
if [ ! -d "$src" ]; then
  echo "Rien à installer ($src/ absent)."
  exit 0
fi

install_dir() { # $1 = sous-dossier de ide-config, $2 = destination
  local from="$src/$1" to="$2"
  [ -d "$from" ] || return 0
  find "$from" -type f | while read -r f; do
    local rel="${f#"$from"/}"
    local dest="$to/$rel"
    mkdir -p "$(dirname "$dest")"
    if [ -e "$dest" ]; then
      echo "déjà présent, ignoré : $dest"
    else
      mv "$f" "$dest"
      echo "installé : $dest"
    fi
  done
}

install_dir vscode .vscode
install_dir cargo .cargo
install_dir claude .claude
install_dir github .github

find "$src" -type d -empty -delete
if [ -d "$src" ]; then
  echo "Des fichiers restent dans $src/ (déjà présents à destination) : à comparer à la main."
else
  echo "OK : configuration installée, $src/ supprimé."
fi
