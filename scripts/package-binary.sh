#!/usr/bin/env bash
set -euo pipefail

version="${1:-}"
if [[ -z "$version" ]]; then
    version="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"
fi

arch="$(uname -m)"
case "$arch" in
    x86_64) ;;
    *)
        echo "unsupported binary release architecture: $arch" >&2
        exit 1
        ;;
esac

asset="glimpse-${version}-${arch}.tar.zst"
pkgroot="dist/pkgroot"

rm -rf "$pkgroot"
mkdir -p \
    "$pkgroot/usr/bin" \
    "$pkgroot/usr/lib/systemd/user" \
    "$pkgroot/etc/pam.d" \
    "$pkgroot/etc/geoclue/conf.d" \
    "$pkgroot/usr/share/xdg-desktop-portal/portals" \
    "$pkgroot/usr/share/dbus-1/services" \
    dist

cargo build --release \
    -p glimpse-lock \
    -p glimpse-shell \
    -p glimpse-idle \
    -p glimpse-sunset \
    -p glimpse-wallpaper

test "$(target/release/glimpse-lock --version)" = "glimpse-lock $version"
test "$(target/release/glimpse-shell --version)" = "glimpse-shell $version"
test "$(target/release/glimpse-idle --version)" = "glimpse-idle $version"
test "$(target/release/glimpse-sunset --version)" = "glimpse-sunset $version"
test "$(target/release/glimpse-wallpaper --version)" = "glimpse-wallpaper $version"

install -Dm755 target/release/glimpse-lock "$pkgroot/usr/bin/glimpse-lock"
install -Dm755 target/release/glimpse-shell "$pkgroot/usr/bin/glimpse-shell"
install -Dm755 target/release/glimpse-idle "$pkgroot/usr/bin/glimpse-idle"
install -Dm755 target/release/glimpse-sunset "$pkgroot/usr/bin/glimpse-sunset"
install -Dm755 target/release/glimpse-wallpaper "$pkgroot/usr/bin/glimpse-wallpaper"
install -Dm644 data/glimpse-lock.service "$pkgroot/usr/lib/systemd/user/glimpse-lock.service"
install -Dm644 data/glimpse-shell.service "$pkgroot/usr/lib/systemd/user/glimpse-shell.service"
install -Dm644 data/glimpse-idle.service "$pkgroot/usr/lib/systemd/user/glimpse-idle.service"
install -Dm644 data/glimpse-sunset.service "$pkgroot/usr/lib/systemd/user/glimpse-sunset.service"
install -Dm644 data/glimpse-wallpaper.service "$pkgroot/usr/lib/systemd/user/glimpse-wallpaper.service"
install -Dm644 data/pam.d/glimpse-lock "$pkgroot/etc/pam.d/glimpse-lock"
install -Dm644 data/geoclue/glimpse.conf "$pkgroot/etc/geoclue/conf.d/glimpse.conf"
install -Dm644 data/portals/glimpse.portal "$pkgroot/usr/share/xdg-desktop-portal/portals/glimpse.portal"
install -Dm644 data/dbus-1/me.aresa.GlimpseIdle.Portal.service "$pkgroot/usr/share/dbus-1/services/me.aresa.GlimpseIdle.Portal.service"

# Ship the bundled theme packs under /usr/share/glimpse/themes/. The shell,
# wallpaper, and lock crates resolve theme packs by name from this root (after
# the user dir $XDG_CONFIG_HOME/glimpse/themes/) so users can pick a packaged
# theme via `theme = "<name>"` in config without copying anything.
themes_dest="$pkgroot/usr/share/glimpse/themes"
for pack in rosepine; do
    if [[ -d "themes/$pack" ]]; then
        install -d "$themes_dest/$pack"
        find "themes/$pack" -mindepth 1 -maxdepth 1 -type f -print0 \
            | xargs -0 -I{} install -Dm644 {} "$themes_dest/$pack/"
    fi
done

applets_dest="$pkgroot/usr/share/glimpse/applets"
scripts/build-glimpse-applets.sh glimpse-applets "$applets_dest"

# Ship applet project templates under /usr/share/glimpse/applet-templates/.
# `glimpse-shell applets new` reads these from disk (not embedded), so the
# binary stays small and templates can be patched without a rebuild.
templates_dest="$pkgroot/usr/share/glimpse/applet-templates"
if [[ -d applet-templates ]]; then
    install -d "$templates_dest"
    find applet-templates -type f -print0 \
        | while IFS= read -r -d '' file; do
            relative="${file#applet-templates/}"
            if [[ -x "$file" ]]; then
                install -Dm755 "$file" "$templates_dest/$relative"
            else
                install -Dm644 "$file" "$templates_dest/$relative"
            fi
        done
fi

if [[ -f LICENSE ]]; then
    install -Dm644 LICENSE "$pkgroot/LICENSE"
fi

tar --zstd -cf "dist/$asset" -C "$pkgroot" .
b2sum "dist/$asset" > "dist/$asset.b2"

echo "dist/$asset"
