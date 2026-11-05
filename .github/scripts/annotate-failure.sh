#!/usr/bin/env bash
# Publie les erreurs d'un journal de construction en annotations GitHub.
# Les annotations sont lisibles sans droits d'administration (API publique
# check-runs/<id>/annotations), contrairement aux journaux complets.
# Usage : annotate-failure.sh <fichier-journal> [titre]
set -u
log="${1:?journal manquant}"
title="${2:-Construction}"
[ -f "$log" ] || { echo "::error title=$title::journal $log absent"; exit 0; }

encode() { sed -e 's/%/%25/g' -e 's/\r//g' | awk '{printf "%s%%0A", $0}'; }

# Lignes d'erreur caractéristiques (rustc, cargo, scripts de build, CMake, clang).
grep -nE "^error(\[E[0-9]+\])?:|error: failed to|panicked at|CMake Error|fatal error:|ld: |No such file|not found" "$log" \
    | head -n 6 | while IFS= read -r line; do
    echo "::error title=$title::$(printf '%s\n' "$line" | encode)"
done
# Contexte : la fin du journal.
echo "::error title=$title (fin du journal)::$(tail -n 60 "$log" | encode)"
