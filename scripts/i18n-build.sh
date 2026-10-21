#!/usr/bin/env bash
set -euo pipefail

read -ra languages <<< "${GLIMPSE_LANGUAGES:?set by the justfile; run through just}"

for lang in "${languages[@]}"; do
    install -d "target/locale/$lang/LC_MESSAGES"
    msgfmt --check --statistics -o "target/locale/$lang/LC_MESSAGES/glimpse.mo" "po/$lang.po"
done
msgfmt --desktop --template data/me.aresa.Glimpse.Shot.desktop -d po -o target/locale/me.aresa.Glimpse.Shot.desktop
msgfmt --desktop --template data/me.aresa.Glimpse.Lens.desktop -d po -o target/locale/me.aresa.Glimpse.Lens.desktop
msgfmt --desktop --template data/me.aresa.Glimpse.Lens.Ruler.desktop -d po -o target/locale/me.aresa.Glimpse.Lens.Ruler.desktop
msgfmt --desktop --template data/me.aresa.Glimpse.Lens.Picker.desktop -d po -o target/locale/me.aresa.Glimpse.Lens.Picker.desktop
msgfmt --desktop --template data/me.aresa.Glimpse.Shot.Annotate.desktop -d po -o target/locale/me.aresa.Glimpse.Shot.Annotate.desktop
