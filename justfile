# glimpse task runner. This is the entry point for every workflow —
# do not invoke cargo directly, add or fix a recipe here instead.

set shell := ["bash", "-uc"]
set positional-arguments

# Single source of truth for install/uninstall/package-binary, passed to those scripts as
# GLIMPSE_BINARIES. Static TOML can't read it, so the cargo-deb/cargo-generate-rpm asset lists
# still hand-duplicate it, as do the scripts' own no-just fallback defaults.
binaries := "glimpsectl glimpse-panel glimpse-lock glimpse-wallpaper glimpse-sunset glimpse-notifications glimpse-weather glimpse-idle"

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
    #!/usr/bin/env bash
    set -uo pipefail
    report=$(blueprint-compiler lint crates/*/blueprints/*.blp 2>&1)
    clean=$(printf '%s\n' "$report" | sed -e 's/\x1b\[[0-9;]*m//g')
    if printf '%s\n' "$clean" | grep -E '^(warning|error)' | grep -qv scrollable_parent; then
        printf '%s\n' "$report"
        exit 1
    fi

[doc("compile every widget example blueprint; the preview does this one at a time")]
check-examples:
    #!/usr/bin/env bash
    set -uo pipefail
    shopt -s nullglob
    status=0
    for blp in var/widget_examples/*.blp; do
        if ! out=$(blueprint-compiler compile --output /dev/null "$blp" 2>&1); then
            printf '%s\n%s\n' "$blp" "$out"
            status=1
        fi
    done
    exit "$status"

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
    #!/usr/bin/env bash
    set -uo pipefail
    paths=({{ PATHS }})
    if [ "${#paths[@]}" -eq 0 ]; then
        shopt -s nullglob
        paths=(crates/*/blueprints/*.blp var/widget_examples/*.blp)
    fi
    blueprint-compiler format -f "${paths[@]}"

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

[doc("one crate's tests including those needing a compositor")]
test-crate-compositor CRATE:
    cargo test -p {{ CRATE }} -- --include-ignored

[doc("everything CI runs")]
verify: fmt-check check lint test check-strings

[doc("search crates.io before writing something by hand")]
search QUERY:
    cargo search "{{ QUERY }}" --limit 20

[doc("validate the shipped systemd units")]
check-units:
    #!/usr/bin/env bash
    set -euo pipefail
    lock=data/systemd/glimpse-lock.service

    noise='is not executable: No such file or directory|^Configuration file .* is marked'
    if systemd-analyze --user verify data/systemd/*.service data/systemd/*.target 2>&1 | grep -Ev "$noise" | grep .; then
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
    members="glimpse-panel glimpse-wallpaper glimpse-sunset glimpse-notifications"
    target=data/systemd/glimpse-session.target
    for member in $members; do
        unit="data/systemd/$member.service"
        grep -qx 'PartOf=glimpse-session.target' "$unit" || {
            echo "$unit: missing PartOf=glimpse-session.target"; exit 1;
        }
        grep -Eq "^Wants=.*${member}\.service" "$target" || {
            echo "$target: missing Wants=$member.service"; exit 1;
        }
        grep -Eq "^PropagatesReloadTo=.*${member}\.service" "$target" || {
            echo "$target: missing PropagatesReloadTo=$member.service"; exit 1;
        }
    done
    if grep -Eq '^(Wants|PropagatesReloadTo)=.*glimpse-lock\.service' "$target"; then
        echo "$target: the on-demand locker must stay outside the suite lifecycle"; exit 1
    fi
    if grep -l '^WantedBy=graphical-session.target$' data/systemd/*.service | grep .; then
        echo "member service is directly enabled by graphical-session.target"; exit 1
    fi
    grep -qx 'WantedBy=graphical-session.target' "$target" || {
        echo "$target: not enabled by graphical-session.target"; exit 1;
    }

    echo "units ok"

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

# ---------------------------------------------------------------- i18n

# Read once here rather than in each recipe: three copies of the same parse is three
# places for a comment or a missing trailing newline to be handled differently.
languages := `grep -vE '^[[:space:]]*(#|$)' po/LINGUAS | tr '\n' ' '`

[doc("regenerate po/glimpse.pot from every translation marker in the tree")]
extract-strings:
    scripts/i18n-extract.sh po/glimpse.pot

[doc("merge po/glimpse.pot into every catalog named by po/LINGUAS")]
update-po: extract-strings
    #!/usr/bin/env bash
    set -euo pipefail
    for lang in {{ languages }}; do
        msgmerge --update --backup=none --previous "po/$lang.po" po/glimpse.pot
    done

[doc("compile po/*.po into target/locale/<lang>/LC_MESSAGES/glimpse.mo")]
build-translations:
    #!/usr/bin/env bash
    set -euo pipefail
    for lang in {{ languages }}; do
        install -d "target/locale/$lang/LC_MESSAGES"
        msgfmt --check --statistics -o "target/locale/$lang/LC_MESSAGES/glimpse.mo" "po/$lang.po"
    done

[doc("fail if the .pot is stale, a catalog is broken, or a blueprint quotes a string xgettext cannot read")]
check-strings:
    #!/usr/bin/env bash
    set -euo pipefail
    fresh="$(mktemp -d)"
    trap 'rm -rf "$fresh"' EXIT
    scripts/i18n-extract.sh "$fresh/glimpse.pot" > /dev/null
    # POT-Creation-Date changes on every run and says nothing about the strings.
    strip() { grep -v '^"POT-Creation-Date:' "$1"; }
    if ! diff -u <(strip po/glimpse.pot) <(strip "$fresh/glimpse.pot"); then
        echo "po/glimpse.pot is stale; run: just extract-strings" >&2
        exit 1
    fi
    for lang in {{ languages }}; do
        [[ -f "po/$lang.po" ]] || { echo "po/LINGUAS names $lang but po/$lang.po is missing" >&2; exit 1; }
        msgfmt --check --output-file=/dev/null "po/$lang.po"
    done
    echo "translations: catalogs current and well-formed"

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
package-deb: build-release-binaries build-translations
    cargo deb -p glimpse-package --no-build

[doc("build a .rpm under target/generate-rpm/ (needs: cargo install cargo-generate-rpm)")]
package-rpm: build-release-binaries build-translations
    cargo generate-rpm -p crates/glimpse-package

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
install: build-release build-translations
    GLIMPSE_BINARIES="{{ binaries }}" scripts/install.sh

[doc("remove installed files")]
uninstall:
    GLIMPSE_BINARIES="{{ binaries }}" scripts/uninstall.sh
