#!/usr/bin/env bash
set -euo pipefail

root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT

source_root="$root/glimpse-applets"
dest_root="$root/pkgroot/usr/share/glimpse/applets"
applet="$source_root/demo"

mkdir -p "$applet/src"
cat >"$applet/applet.toml" <<'TOML'
id = "demo"
type = "exec"

[exec]
command = ["/usr/share/glimpse/applets/demo/demo"]
TOML

cat >"$applet/Cargo.toml" <<'TOML'
[package]
name = "demo"
version = "0.1.0"
edition = "2024"

[workspace]
TOML

cat >"$applet/src/main.rs" <<'RS'
fn main() {
    println!("demo");
}
RS

cargo generate-lockfile --manifest-path "$applet/Cargo.toml" >/dev/null

scripts/build-glimpse-applets.sh "$source_root" "$dest_root"

test -f "$dest_root/demo/applet.toml"
test -x "$dest_root/demo/demo"
grep -q '/usr/share/glimpse/applets/demo/demo' "$dest_root/demo/applet.toml"
