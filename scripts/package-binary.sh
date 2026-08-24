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
# GLIMPSE_BINARIES is set by `just package-binary` from the justfile's single source of
# truth; the fallback here only matters for a direct, non-just invocation of this script.
read -ra binaries <<< "${GLIMPSE_BINARIES:-glimpsectl glimpsed glimpse-panel glimpse-lock glimpse-wallpaper glimpse-sunset}"

rm -rf "$pkgroot"
mkdir -p \
    "$pkgroot/usr/bin" \
    "$pkgroot/usr/lib/systemd/user" \
    "$pkgroot/usr/share/dbus-1/services" \
    "$pkgroot/usr/share/glimpse/wallpapers" \
    "$pkgroot/etc/pam.d" \
    dist

cargo_args=()
for bin in "${binaries[@]}"; do
    cargo_args+=(-p "$bin")
done
cargo build --release "${cargo_args[@]}"

for bin in "${binaries[@]}"; do
    # shadow-rs's --version output is multi-line (pkg_version:X.Y.Z on its own line),
    # not the plain clap "name version" this compared against before.
    actual_version="$(target/release/"$bin" --version | sed -n 's/^pkg_version://p')"
    if [[ "$actual_version" != "$version" ]]; then
        echo "error: $bin --version reports pkg_version '$actual_version', expected '$version'" >&2
        exit 1
    fi
    install -Dm755 "target/release/$bin" "$pkgroot/usr/bin/$bin"
done

install -Dm644 data/config.default.toml "$pkgroot/usr/share/glimpse/config.default.toml"
install -Dm644 data/config.schema.json "$pkgroot/usr/share/glimpse/config.schema.json"
install -Dm644 LICENSE "$pkgroot/usr/share/glimpse/LICENSE"

for f in data/systemd/*.service; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$pkgroot/usr/lib/systemd/user/$(basename "$f")"
done
for f in data/dbus-1/services/*.service; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$pkgroot/usr/share/dbus-1/services/$(basename "$f")"
done
for f in data/pam.d/*; do
    [[ -e "$f" && "$(basename "$f")" != .gitkeep ]] && install -Dm644 "$f" "$pkgroot/etc/pam.d/$(basename "$f")"
done
for f in wallpapers/*; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$pkgroot/usr/share/glimpse/wallpapers/$(basename "$f")"
done

tar --zstd -cf "dist/$asset" -C "$pkgroot" .
b2sum "dist/$asset" > "dist/$asset.b2"
sha256sum "dist/$asset" > "dist/$asset.sha256"

echo "dist/$asset"
