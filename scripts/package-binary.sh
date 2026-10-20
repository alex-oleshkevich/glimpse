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
PREFIX=/usr DESTDIR="$pkgroot" source scripts/glimpse-paths.sh
read -ra binaries <<< "${GLIMPSE_BINARIES:?set by the justfile; run through just}"

rm -rf "$pkgroot"
mkdir -p \
    "$pkgroot/usr/bin" \
    "$pkgroot/usr/lib/systemd/user" \
    "$pkgroot/usr/share/dbus-1/services" \
    "$pkgroot/usr/share/xdg-desktop-portal/portals" \
    "$pkgroot/etc/geoclue/conf.d" \
    dist

cargo_args=()
for bin in "${binaries[@]}"; do
    cargo_args+=(-p "$bin")
done
cargo build --release "${cargo_args[@]}"

for bin in "${binaries[@]}"; do
    actual_version="$(target/release/"$bin" --version | sed -n -e 's/^pkg_version://p' -e "s/^$bin \([0-9].*\)$/\1/p")"
    if [[ "$actual_version" != "$version" ]]; then
        echo "error: $bin --version reports '$actual_version', expected '$version'" >&2
        exit 1
    fi
    install -Dm755 "target/release/$bin" "$pkgroot/usr/bin/$bin"
done

install -Dm755 data/bin/glimpse-dpms "$pkgroot/usr/bin/glimpse-dpms"
install -Dm644 data/config.default.toml "$pkgroot/usr/share/glimpse/config.default.toml"
install -Dm644 data/config.commented.toml "$pkgroot/usr/share/glimpse/config.commented.toml"
install -Dm644 data/config.schema.json "$pkgroot/usr/share/glimpse/config.schema.json"
install -Dm644 data/language-codes.json "$pkgroot/usr/share/glimpse/language-codes.json"
install -Dm644 LICENSE "$pkgroot/usr/share/glimpse/LICENSE"
install -Dm644 target/locale/me.aresa.Glimpse.Shot.desktop "$applicationsdir/me.aresa.Glimpse.Shot.desktop"
install -Dm644 target/locale/me.aresa.Glimpse.Lens.desktop "$applicationsdir/me.aresa.Glimpse.Lens.desktop"
install -Dm644 target/locale/me.aresa.Glimpse.Lens.Ruler.desktop "$applicationsdir/me.aresa.Glimpse.Lens.Ruler.desktop"
install -Dm644 target/locale/me.aresa.Glimpse.Lens.Picker.desktop "$applicationsdir/me.aresa.Glimpse.Lens.Picker.desktop"
install -Dm644 target/locale/me.aresa.Glimpse.Shot.Annotate.desktop "$applicationsdir/me.aresa.Glimpse.Shot.Annotate.desktop"
install -Dm644 data/me.aresa.Glimpse.Shot.metainfo.xml "$pkgroot/usr/share/metainfo/me.aresa.Glimpse.Shot.metainfo.xml"
install -Dm644 data/icons/hicolor/scalable/apps/me.aresa.Glimpse.Shot.svg "$pkgroot/usr/share/icons/hicolor/scalable/apps/me.aresa.Glimpse.Shot.svg"
install -Dm644 data/icons/hicolor/symbolic/apps/me.aresa.Glimpse.Shot-symbolic.svg "$pkgroot/usr/share/icons/hicolor/symbolic/apps/me.aresa.Glimpse.Shot-symbolic.svg"
install -Dm644 data/icons/hicolor/scalable/apps/me.aresa.Glimpse.Lens.svg "$pkgroot/usr/share/icons/hicolor/scalable/apps/me.aresa.Glimpse.Lens.svg"
install -Dm644 data/icons/hicolor/scalable/apps/me.aresa.Glimpse.Shot.Annotate.svg "$pkgroot/usr/share/icons/hicolor/scalable/apps/me.aresa.Glimpse.Shot.Annotate.svg"
install -Dm644 data/icons/hicolor/symbolic/apps/me.aresa.Glimpse.Lens-symbolic.svg "$pkgroot/usr/share/icons/hicolor/symbolic/apps/me.aresa.Glimpse.Lens-symbolic.svg"
install -Dm644 data/icons/hicolor/scalable/apps/me.aresa.Glimpse.Lens.Ruler.svg "$pkgroot/usr/share/icons/hicolor/scalable/apps/me.aresa.Glimpse.Lens.Ruler.svg"
install -Dm644 data/icons/hicolor/symbolic/apps/me.aresa.Glimpse.Lens.Ruler-symbolic.svg "$pkgroot/usr/share/icons/hicolor/symbolic/apps/me.aresa.Glimpse.Lens.Ruler-symbolic.svg"
install -Dm644 data/icons/hicolor/scalable/apps/me.aresa.Glimpse.Lens.Picker.svg "$pkgroot/usr/share/icons/hicolor/scalable/apps/me.aresa.Glimpse.Lens.Picker.svg"
install -Dm644 data/icons/hicolor/symbolic/apps/me.aresa.Glimpse.Lens.Picker-symbolic.svg "$pkgroot/usr/share/icons/hicolor/symbolic/apps/me.aresa.Glimpse.Lens.Picker-symbolic.svg"
install -Dm644 data/icons/hicolor/scalable/apps/me.aresa.Glimpse.svg "$pkgroot/usr/share/icons/hicolor/scalable/apps/me.aresa.Glimpse.svg"
install -Dm644 data/icons/hicolor/symbolic/apps/me.aresa.Glimpse-symbolic.svg "$pkgroot/usr/share/icons/hicolor/symbolic/apps/me.aresa.Glimpse-symbolic.svg"

while IFS= read -r -d '' f; do
    install -Dm644 "$f" "$pkgroot/usr/share/glimpse/$f"
done < <(find sdk/templates -type f -print0)

for f in data/systemd/*.service; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$pkgroot/usr/lib/systemd/user/$(basename "$f")"
done
for f in data/systemd/*.target; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$pkgroot/usr/lib/systemd/user/$(basename "$f")"
done
for f in data/geoclue/conf.d/*.conf; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$pkgroot/etc/geoclue/conf.d/$(basename "$f")"
done
for f in data/dbus-1/services/*.service; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$pkgroot/usr/share/dbus-1/services/$(basename "$f")"
done
for f in data/portals/*.portal; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$pkgroot/usr/share/xdg-desktop-portal/portals/$(basename "$f")"
done
for f in data/portals/*-portals.conf; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$pkgroot/etc/xdg-desktop-portal/$(basename "$f")"
    [[ -e "$f" ]] && install -Dm644 "$f" "$pkgroot/usr/share/glimpse/portals/$(basename "$f")"
done
for f in data/pam.d/*; do
    [[ -f "$f" ]] || continue
    install -Dm644 "$f" "$pkgroot/etc/pam.d/$(basename "$f")"
done
for f in data/themes/*/*; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$pkgroot/usr/share/glimpse/themes/$(basename "$(dirname "$f")")/$(basename "$f")"
done

for f in target/locale/*/LC_MESSAGES/glimpse.mo; do
    [[ -e "$f" ]] || continue
    lang="$(basename "$(dirname "$(dirname "$f")")")"
    install -Dm644 "$f" "$pkgroot/usr/share/locale/$lang/LC_MESSAGES/glimpse.mo"
done

tar --zstd -cf "dist/$asset" -C "$pkgroot" .
b2sum "dist/$asset" > "dist/$asset.b2"
sha256sum "dist/$asset" > "dist/$asset.sha256"

echo "dist/$asset"
