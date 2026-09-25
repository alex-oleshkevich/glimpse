#!/usr/bin/env bash
set -euo pipefail

status=0
for entry in data/*.desktop target/locale/*.desktop; do
    [[ -e "$entry" ]] || continue
    desktop-file-validate "$entry" || status=1
done
exit "$status"
