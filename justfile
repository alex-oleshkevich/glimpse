# glimpse task runner. This is the entry point for every workflow —
# do not invoke cargo directly, add or fix a recipe here instead.

set shell := ["bash", "-uc"]

# Single source of truth for install/uninstall/package-binary, passed to those scripts as
# GLIMPSE_BINARIES. Static TOML can't read it, so the cargo-deb/cargo-generate-rpm asset lists
# still hand-duplicate it, as do the scripts' own no-just fallback defaults.
binaries := "glimpsectl glimpsed glimpse-panel glimpse-lock glimpse-wallpaper glimpse-sunset"

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
    blueprint-compiler lint crates/*/blueprints/*.blp

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

[doc("regenerate data/config.default.toml from Config::default()")]
gen-config-default:
    cargo run -q -p glimpse-config --example gen_config_default > data/config.default.toml

[doc("regenerate data/config.schema.json from the Config types")]
gen-config-schema:
    cargo run -q -p glimpse-config --example gen_config_schema > data/config.schema.json

[doc("headless tests")]
test:
    cargo test --workspace

[doc("test one crate")]
test-crate CRATE:
    cargo test -p {{ CRATE }}

[doc("all tests including those needing a compositor")]
test-compositor:
    cargo test --workspace -- --include-ignored

[doc("everything CI runs")]
verify: fmt-check check lint test

[doc("search crates.io before writing something by hand")]
search QUERY:
    cargo search "{{ QUERY }}" --limit 20

[doc("validate the shipped systemd units")]
check-units:
    #!/usr/bin/env bash
    set -euo pipefail
    lock=data/systemd/glimpse-lock.service

    noise='is not executable: No such file or directory|^Configuration file .* is marked'
    if systemd-analyze --user verify data/systemd/*.service 2>&1 | grep -Ev "$noise" | grep .; then
        exit 1
    fi

    for f in data/systemd/*.service; do
        bin=$(grep -m1 -oE '^ExecStart=[^ ]+' "$f" | sed 's|.*/||')
        case " {{ binaries }} " in
            *" $bin "*) ;;
            *) echo "$f: ExecStart names '$bin', which is not a shipped binary"; exit 1 ;;
        esac
    done

    for key in $(sed -n '/^\[Service\]/,/^\[/p' "$lock" | grep -oE '^[A-Za-z]+=' | tr -d '='); do
        case " Type ExecStart ExecReload Restart RestartSec " in
            *" $key "*) ;;
            *) echo "$lock: [Service] carries $key= — sandboxing breaks PAM, see README"; exit 1 ;;
        esac
    done

    if grep -qE '^(BindsTo|Conflicts|Requires|Requisite)=' "$lock"; then
        echo "$lock: a Requires-class or Conflicts= edge can stop the locker mid-lock"; exit 1
    fi
    if grep -E '^PartOf=' "$lock" | grep -qv '^PartOf=graphical-session.target$'; then
        echo "$lock: PartOf= anything but graphical-session.target can stop the locker mid-lock"; exit 1
    fi
    if grep -l '^Requires=.*glimpsed' data/systemd/*.service | grep .; then
        echo "Requires=glimpsed.service — use Wants="; exit 1
    fi

    echo "units ok"

# ---------------------------------------------------------------- run

[doc("run daemon")]
run-daemon *ARGS:
    cargo run -p glimpsed -- {{ ARGS }}

[doc("run panel")]
run-panel *ARGS:
    cargo run -p glimpse-panel -- {{ ARGS }}

[doc("run wallpaper")]
run-wallpaper *ARGS:
    cargo run -p glimpse-wallpaper -- {{ ARGS }}

[doc("run locker")]
run-locker *ARGS:
    cargo run -p glimpse-lock -- {{ ARGS }}

[doc("run sunset")]
run-sunset *ARGS:
    cargo run -p glimpse-sunset -- {{ ARGS }}

[doc("run the CLI")]
ctl *ARGS:
    cargo run -q -p glimpsectl -- {{ ARGS }}

[doc("nested niri in a window; run the panel inside it for a fast dev loop")]
nested:
    niri

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
    #!/usr/bin/env bash
    set -euo pipefail
    args=()
    for b in {{ binaries }}; do args+=(-p "$b"); done
    cargo build --profile profiling "${args[@]}"

[doc("build only the shipped binaries, release — unlike build-release, doesn't need every workspace crate to compile")]
build-release-binaries:
    #!/usr/bin/env bash
    set -euo pipefail
    args=()
    for b in {{ binaries }}; do args+=(-p "$b"); done
    cargo build --release "${args[@]}"

# ---------------------------------------------------------------- package

[doc("fail unless TAG (e.g. v0.16.0) matches workspace.package.version in Cargo.toml")]
release-verify TAG:
    #!/usr/bin/env bash
    set -euo pipefail
    version="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"
    raw_tag={{ quote(TAG) }}
    tag="${raw_tag#v}"
    if [ "$tag" != "$version" ]; then
        echo "tag ${raw_tag} does not match Cargo.toml version $version" >&2
        exit 1
    fi
    echo "tag ${raw_tag} matches Cargo.toml version $version"

[doc("build a release tarball (glimpse-<version>-<arch>.tar.zst) under dist/ — builds its own binaries")]
package-binary VERSION="":
    GLIMPSE_BINARIES="{{ binaries }}" scripts/package-binary.sh {{ quote(VERSION) }}

[doc("build a .deb under target/debian/ (needs: cargo install cargo-deb)")]
package-deb: build-release-binaries
    cargo deb -p glimpsed --no-build

[doc("build a .rpm under target/generate-rpm/ (needs: cargo install cargo-generate-rpm)")]
package-rpm: build-release-binaries
    cargo generate-rpm -p crates/glimpsed

[doc("render dist/PKGBUILD for VERSION with the x86_64 tarball's b2sum patched in")]
aur-pkgbuild VERSION B2SUM:
    mkdir -p dist
    scripts/render-aur-pkgbuild.sh {{ quote(VERSION) }} {{ quote(B2SUM) }} > dist/PKGBUILD

# ---------------------------------------------------------------- clean

[doc("remove the whole target directory")]
clean:
    cargo clean

[doc("remove artifacts for one crate")]
clean-crate CRATE:
    cargo clean -p {{ CRATE }}

# ---------------------------------------------------------------- install

[doc("install binaries and data, honours PREFIX and DESTDIR")]
install: build-release
    GLIMPSE_BINARIES="{{ binaries }}" scripts/install.sh

[doc("remove installed files")]
uninstall:
    GLIMPSE_BINARIES="{{ binaries }}" scripts/uninstall.sh
