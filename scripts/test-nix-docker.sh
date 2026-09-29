#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" != --inside ]]; then
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    test -c /dev/kvm
    docker run --rm --device=/dev/kvm \
        --mount type=volume,src=glimpse-nix-store,dst=/nix \
        --mount "type=bind,src=$root,dst=/src,readonly" \
        -w /src \
        'nixos/nix@sha256:7a007c766426c1877758ddc5cb87a965ac131fc78c582ce0083d922d51ae945c' \
        bash /src/scripts/test-nix-docker.sh --inside
    exit 0
fi

nix_flags=(--extra-experimental-features 'nix-command flakes')
build_flags=(--option sandbox false --option system-features 'kvm nixos-test' --option cores 2 --option max-jobs 1)
output="$(nix "${nix_flags[@]}" build 'path:/src#default' --no-link --print-out-paths "${build_flags[@]}")"

for binary in glimpsectl glimpse-panel glimpse-lock glimpse-wallpaper glimpse-sunset glimpse-notifications glimpse-weather glimpse-idle glimpse-picker glimpse-ruler glimpse-shot; do
    test -x "$output/bin/$binary"
    "$output/bin/$binary" --version >/dev/null
done

for file in "$output/bin/glimpse-dpms" "$output/share/glimpse/config.default.toml" "$output/share/glimpse/config.commented.toml" "$output/share/glimpse/config.schema.json" "$output/share/glimpse/themes/adwaita/default.jpg" "$output/share/glimpse/sdk/applet/examples/todo/main.tsx" "$output/share/locale/ru/LC_MESSAGES/glimpse.mo" "$output/lib/systemd/user/glimpse-session.target" "$output/share/dbus-1/services/me.aresa.Glimpse.Idle.service" "$output/share/xdg-desktop-portal/portals/glimpse.portal" "$output/share/xdg-desktop-portal/portals/glimpse-shot.portal" "$output/share/dbus-1/services/me.aresa.Glimpse.Shot.service" "$output/share/applications/me.aresa.Glimpse.Shot.desktop" "$output/lib/systemd/user/glimpse-desktop.service"; do
    test -e "$file"
done

if grep -R -E '/usr/bin/glimpse|/usr/share/glimpse' "$output/lib/systemd/user" "$output/share/dbus-1/services" "$output/share/applications" "$output/share/glimpse"; then
    exit 1
fi

nix "${nix_flags[@]}" flake check path:/src "${build_flags[@]}"
