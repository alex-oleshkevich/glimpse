#!/usr/bin/env bash
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/glimpse-paths.sh"

for b in "${binaries[@]}"; do
    install -Dm755 "target/release/$b" "$bindir/$b"
done

for f in data/systemd/*.service; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$unitdir/$(basename "$f")"
done
for f in data/dbus-1/services/*.service; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$dbusdir/$(basename "$f")"
done
for f in data/pam.d/*; do
    [[ -e "$f" && "$(basename "$f")" != .gitkeep ]] && install -Dm644 "$f" "$pamdir/$(basename "$f")"
done
for f in data/geoclue/conf.d/*.conf; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$geocluedir/$(basename "$f")"
done

install -Dm644 data/config.default.toml "$sharedir/config.default.toml"
install -Dm644 data/config.schema.json "$sharedir/config.schema.json"
install -Dm644 data/language-codes.json "$sharedir/language-codes.json"
install -Dm644 LICENSE "$sharedir/LICENSE"

for f in data/themes/*/*.css; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$sharedir/themes/$(basename "$(dirname "$f")")/$(basename "$f")"
done
for f in wallpapers/*; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$sharedir/wallpapers/$(basename "$f")"
done
