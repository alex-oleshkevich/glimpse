# glimpse task runner. This is the entry point for every workflow —
# do not invoke cargo directly, add or fix a recipe here instead.

set shell := ["bash", "-uc"]
set positional-arguments

# Single source of truth for every script that walks the shipped binaries, passed to them as
# GLIMPSE_BINARIES. Static TOML can't read it, so the cargo-deb/cargo-generate-rpm asset lists
# still hand-duplicate it, as do the scripts' own no-just fallback defaults.
# How a recipe that writes outside the tree becomes root. `sudo` because these are run from a
# terminal that has one; set GLIMPSE_SUDO=pkexec from a launcher that does not.
elevate := if env("GLIMPSE_SUDO", "") != "" { env("GLIMPSE_SUDO", "") } else { if `id -u` == "0" { "" } else { "sudo" } }

binaries := "glimpsectl glimpse-panel glimpse-lock glimpse-wallpaper glimpse-sunset glimpse-notifications glimpse-weather glimpse-idle glimpse-picker glimpse-applet glimpse-ruler"

[doc("list recipes")]
default:
    @just --list --unsorted

# ---------------------------------------------------------------- verify

[doc("type-check the workspace")]
check:
    cargo check --workspace --all-targets

[doc("type-check one crate")]
check-crate CRATE:
    cargo check -p {{ CRATE }} --all-targets

[doc("rust, systemd units and blueprints; warnings are errors")]
lint: lint-rust check-units lint-blueprints

[doc("clippy on the workspace, warnings are errors")]
lint-rust:
    cargo clippy --workspace --all-targets -- -D warnings

[doc("blueprint templates")]
lint-blueprints:
    scripts/lint-blueprints.sh

[doc("compile every widget example blueprint; the preview does this one at a time")]
check-examples:
    scripts/check-examples.sh

[doc("clippy on one crate")]
lint-crate CRATE:
    cargo clippy -p {{ CRATE }} --all-targets -- -D warnings

[doc("format in place")]
fmt:
    cargo fmt --all

[doc("format one crate in place")]
fmt-crate CRATE:
    cargo fmt -p {{ CRATE }}

[doc("fail if anything is unformatted")]
fmt-check:
    cargo fmt --all --check

[doc("format blueprints in place; pass paths, or all of them by default")]
fmt-blueprints *PATHS:
    scripts/fmt-blueprints.sh "$@"

[doc("regenerate data/config.default.toml from Config::default()")]
gen-config-default:
    cargo run -q -p glimpse-config --example gen_config_default > data/config.default.toml

[doc("regenerate data/config.schema.json from the Config types")]
gen-config-schema:
    cargo run -q -p glimpse-config --example gen_config_schema > data/config.schema.json

[doc("regenerate data/config.commented.toml, the seed installed into ~/.config/glimpse")]
gen-config-commented:
    cargo run -q -p glimpse-config --example gen_config_commented > data/config.commented.toml

[doc("headless tests")]
test:
    cargo test --workspace

[doc("test one crate")]
test-crate CRATE:
    cargo test -p {{ CRATE }}

[doc("all tests including those needing a compositor; each ignored test runs in its own process")]
test-compositor:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ]; then
        echo "test-compositor: neither WAYLAND_DISPLAY nor DISPLAY is set; every GTK test would return early and pass without running" >&2
        exit 1
    fi
    cargo test --workspace
    failed=()
    for crate in $(cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name'); do
        names=$(cargo test -p "$crate" -- --list --ignored 2>/dev/null | grep -E ': test$' | sed 's/: test$//') || true
        [ -z "$names" ] && continue
        while IFS= read -r name; do
            [ -z "$name" ] && continue
            echo "==> $crate :: $name"
            if ! cargo test -p "$crate" "$name" -- --ignored --exact; then
                failed+=("$crate :: $name")
            fi
        done <<< "$names"
    done
    if [ "${#failed[@]}" -gt 0 ]; then
        echo "test-compositor: failed ignored tests:" >&2
        printf '  %s\n' "${failed[@]}" >&2
        exit 1
    fi

[doc("one crate's tests including those needing a compositor; each ignored test runs in its own process; FILTER runs only the matching ignored tests")]
test-crate-compositor CRATE FILTER="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ]; then
        echo "test-crate-compositor: neither WAYLAND_DISPLAY nor DISPLAY is set; every GTK test would return early and pass without running" >&2
        exit 1
    fi
    if [ -z "{{ FILTER }}" ]; then
        cargo test -p {{ CRATE }}
    fi
    names=$(cargo test -p {{ CRATE }} -- --list --ignored 2>/dev/null | grep -E ': test$' | sed 's/: test$//') || true
    if [ -n "{{ FILTER }}" ]; then
        names=$(printf '%s\n' "$names" | grep -F -- "{{ FILTER }}") || true
    fi
    failed=()
    while IFS= read -r name; do
        [ -z "$name" ] && continue
        echo "==> {{ CRATE }} :: $name"
        if ! cargo test -p {{ CRATE }} "$name" -- --ignored --exact; then
            failed+=("$name")
        fi
    done <<< "$names"
    if [ "${#failed[@]}" -gt 0 ]; then
        echo "test-crate-compositor: failed ignored tests:" >&2
        printf '  %s\n' "${failed[@]}" >&2
        exit 1
    fi

[doc("one test by its full path, ignored or not; a GTK test only proves anything run alone")]
test-one CRATE TEST:
    cargo test -p {{ CRATE }} -- --include-ignored --exact {{ TEST }}

[doc("everything CI runs")]
verify: fmt-check check lint test check-strings

[doc("run the applet SDK tests")]
sdk-test:
    "${GLIMPSE_DENO:-deno}" test --allow-read --allow-env --config sdk/applet/deno.json sdk/applet

[doc("search crates.io before writing something by hand")]
search QUERY:
    cargo search "{{ QUERY }}" --limit 20

[doc("validate the shipped systemd units")]
check-units:
    GLIMPSE_BINARIES="{{ binaries }}" scripts/check-units.sh

# ---------------------------------------------------------------- run

[doc("run panel")]
run-panel *ARGS:
    cargo run -p glimpse-panel -- "$@"

[doc("run wallpaper")]
run-wallpaper *ARGS:
    cargo run -p glimpse-wallpaper -- "$@"

[doc("run locker")]
run-locker *ARGS:
    cargo run -p glimpse-lock -- "$@"

[doc("run sunset")]
run-sunset *ARGS:
    cargo run -p glimpse-sunset -- "$@"

[doc("run idle provider")]
run-idle *ARGS:
    cargo run -p glimpse-idle -- "$@"

[doc("run notification popups")]
run-notifications *ARGS:
    cargo run -p glimpse-notifications -- "$@"

[doc("run weather provider")]
run-weather *ARGS:
    cargo run -p glimpse-weather -- "$@"

[doc("run the color picker: pick a color from the screen and print it")]
run-picker *ARGS:
    cargo run -p glimpse-picker -- "$@"

[doc("run the screen ruler: measure distances on the screen and print each segment")]
run-ruler *ARGS:
    cargo run -p glimpse-ruler -- "$@"

[doc("run the CLI")]
ctl *ARGS:
    cargo run -q -p glimpsectl -- "$@"

[doc("network test safety net: baseline | restore | arm [s] | disarm | status")]
net-guard *ARGS:
    scripts/net-guard.sh {{ ARGS }}

[doc("click a point on a niri output with ydotool")]
click *ARGS:
    python3 scripts/click.py {{ ARGS }}

[doc("render one blueprint with the real widgets; reloads on save")]
preview BLUEPRINT *ARGS:
    cargo run -q -p glimpse-widgets --example preview -- {{ BLUEPRINT }} {{ ARGS }}

[doc("nested niri in a window; run the panel inside it for a fast dev loop")]
nested:
    niri

[doc("two fake tray items on whatever bus DBUS_SESSION_BUS_ADDRESS names")]
fake-tray:
    cargo run -q -p glimpse-dbus --features testing --example fake-tray

# ---------------------------------------------------------------- build

[doc("build all, debug")]
build:
    cargo build --workspace

[doc("build all, release")]
build-release:
    cargo build --workspace --release

[doc("build one crate")]
build-crate CRATE:
    cargo build -p {{ CRATE }}

[doc("build the shipped binaries with symbols, for perf; output in target/profiling/")]
build-profiling:
    GLIMPSE_BINARIES="{{ binaries }}" scripts/build-binaries.sh --profile profiling

[doc("build only the shipped binaries, release — unlike build-release, doesn't need every workspace crate to compile")]
build-release-binaries:
    GLIMPSE_BINARIES="{{ binaries }}" scripts/build-binaries.sh --release

# ---------------------------------------------------------------- i18n

# Read once here rather than in each recipe: three copies of the same parse is three
# places for a comment or a missing trailing newline to be handled differently.
languages := `grep -vE '^[[:space:]]*(#|$)' po/LINGUAS | tr '\n' ' '`

[doc("regenerate po/glimpse.pot from every translation marker in the tree")]
extract-strings:
    scripts/i18n-extract.sh po/glimpse.pot

[doc("merge po/glimpse.pot into every catalog named by po/LINGUAS")]
update-po: extract-strings
    GLIMPSE_LANGUAGES="{{ languages }}" scripts/i18n-update-po.sh

[doc("compile po/*.po into target/locale/<lang>/LC_MESSAGES/glimpse.mo")]
build-translations:
    GLIMPSE_LANGUAGES="{{ languages }}" scripts/i18n-build.sh

[doc("fail if the .pot is stale, a catalog is broken, or a blueprint quotes a string xgettext cannot read")]
check-strings:
    GLIMPSE_LANGUAGES="{{ languages }}" scripts/i18n-check.sh

# ---------------------------------------------------------------- package

[doc("fail unless TAG (e.g. v0.16.0) matches workspace.package.version in Cargo.toml")]
release-verify TAG:
    scripts/release-verify.sh "$1"

[doc("build a release tarball (glimpse-<version>-<arch>.tar.zst) under dist/ — builds its own binaries")]
package-binary VERSION="": build-translations
    GLIMPSE_BINARIES="{{ binaries }}" scripts/package-binary.sh {{ quote(VERSION) }}

[doc("build a .deb under target/debian/ (needs: cargo install cargo-deb)")]
package-deb: build-release-binaries build-translations
    cargo deb -p glimpse-package --no-build

[doc("build the Fedora and openSUSE .rpm under target/generate-rpm/ (needs: cargo install cargo-generate-rpm)")]
package-rpm: build-release-binaries build-translations
    cargo generate-rpm -p crates/glimpse-package
    cargo generate-rpm -p crates/glimpse-package --variant opensuse

[doc("build an Arch package under dist/ (needs: base-devel) — builds its own binaries")]
package-aur: package-binary
    scripts/package-aur.sh

[doc("render dist/PKGBUILD for the AUR: VERSION and the released tarball's b2sum")]
release-pkgbuild VERSION B2SUM:
    mkdir -p dist
    scripts/render-pkgbuild.sh --release {{ quote(VERSION) }} {{ quote(B2SUM) }} > dist/PKGBUILD

# ---------------------------------------------------------------- clean

[doc("remove the whole target directory")]
clean:
    cargo clean

[doc("remove artifacts for one crate")]
clean-crate CRATE:
    cargo clean -p {{ CRATE }}

# ---------------------------------------------------------------- install

[doc("install binaries and data, honours PREFIX and DESTDIR")]
install: build-release build-translations
    GLIMPSE_BINARIES="{{ binaries }}" scripts/install.sh

[doc("remove installed files")]
uninstall:
    GLIMPSE_BINARIES="{{ binaries }}" scripts/uninstall.sh

[doc("build the Arch package and install it with pacman (elevate with $GLIMPSE_SUDO, default sudo)")]
install-aur: package-aur
    GLIMPSE_ELEVATE="{{ elevate }}" scripts/install-aur.sh

[doc("remove the installed Arch package (elevate with $GLIMPSE_SUDO, default sudo)")]
uninstall-aur:
    GLIMPSE_ELEVATE="{{ elevate }}" scripts/uninstall-aur.sh
