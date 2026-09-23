#!/usr/bin/env bash
set -euo pipefail

read -ra languages <<< "${GLIMPSE_LANGUAGES:?set by the justfile; run through just}"

fresh="$(mktemp -d)"
trap 'rm -rf "$fresh"' EXIT
scripts/i18n-extract.sh "$fresh/glimpse.pot" > /dev/null
# POT-Creation-Date changes on every run and says nothing about the strings.
strip() { grep -v '^"POT-Creation-Date:' "$1"; }
if ! diff -u <(strip po/glimpse.pot) <(strip "$fresh/glimpse.pot"); then
    echo "po/glimpse.pot is stale; run: just extract-strings" >&2
    exit 1
fi
for lang in "${languages[@]}"; do
    [[ -f "po/$lang.po" ]] || { echo "po/LINGUAS names $lang but po/$lang.po is missing" >&2; exit 1; }
    msgfmt --check --output-file=/dev/null "po/$lang.po"
done
echo "translations: catalogs current and well-formed"
