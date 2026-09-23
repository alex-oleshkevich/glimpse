#!/usr/bin/env bash
set -euo pipefail

read -ra languages <<< "${GLIMPSE_LANGUAGES:?set by the justfile; run through just}"

for lang in "${languages[@]}"; do
    msgmerge --update --backup=none --previous "po/$lang.po" po/glimpse.pot
done
