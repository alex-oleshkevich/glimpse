# glimpse task runner. This is the entry point for every workflow —
# do not invoke cargo directly, add or fix a recipe here instead.

set shell := ["bash", "-uc"]
set positional-arguments

# Single source of truth for every script that walks the shipped binaries, passed to them as
# GLIMPSE_BINARIES. Static TOML can't read it, so the cargo-deb/cargo-generate-rpm asset lists
# still hand-duplicate it.
binaries := "glimpsectl glimpse-panel glimpse-lock glimpse-wallpaper glimpse-sunset glimpse-notifications glimpse-agents glimpse-weather glimpse-calendar glimpse-idle glimpse-lens glimpse-shot glimpse-settings-daemon"

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

[doc("rust, systemd units, blueprints and desktop entries; warnings are errors")]
lint: lint-rust check-units lint-blueprints lint-desktop

[doc("clippy on the workspace, warnings are errors")]
lint-rust:
    cargo clippy --workspace --all-targets -- -D warnings

[doc("blueprint templates")]
lint-blueprints:
    scripts/lint-blueprints.sh

[doc("desktop-file-validate on every desktop entry in data/")]
lint-desktop:
    scripts/lint-desktop.sh

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

[doc("regenerate data/DOC.default.toml (DOC is config or panel) from Config::default()")]
gen-config-default DOC:
    cargo run -q -p glimpse-config --example gen_config_default {{ DOC }} > data/{{ DOC }}.default.toml

[doc("regenerate data/DOC.schema.json (DOC is config or panel) from the Config types")]
gen-config-schema DOC:
    cargo run -q -p glimpse-config --example gen_config_schema {{ DOC }} > data/{{ DOC }}.schema.json

[doc("regenerate data/DOC.commented.toml (DOC is config or panel), the reference shipped beside the schema")]
gen-config-commented DOC:
    cargo run -q -p glimpse-config --example gen_config_commented {{ DOC }} > data/{{ DOC }}.commented.toml

[doc("headless tests")]
test:
    cargo test --workspace

[doc("test one crate")]
test-crate CRATE:
    cargo test -p {{ CRATE }}

[doc("all tests including those needing a compositor; each ignored test runs in its own process")]
test-compositor:
    scripts/test-compositor.sh

[doc("one crate's tests including those needing a compositor; each ignored test runs in its own process; FILTER runs only the matching ignored tests")]
test-crate-compositor CRATE FILTER="":
    scripts/test-compositor.sh "$@"

[doc("one test by its full path, ignored or not; a GTK test only proves anything run alone")]
test-one CRATE TEST:
    cargo test -p {{ CRATE }} -- --include-ignored --exact {{ TEST }}

[doc("everything CI runs")]
verify: fmt-check check lint test check-strings

[doc("run the applet SDK tests")]
sdk-test:
    scripts/sdk-test.sh

[doc("publish glimpse-sdk to npm; extra arguments go to npm publish, e.g. --dry-run")]
[working-directory("sdk/sdk-ts")]
sdk-publish *ARGS: sdk-test
    npm publish "$@"

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

[doc("run calendar provider")]
run-calendar *ARGS:
    cargo run -p glimpse-calendar -- "$@"

[doc("run the lens tool; --tool picker|ruler picks the tool")]
run-lens *ARGS:
    cargo run -p glimpse-lens -- "$@"

[doc("run the desktop settings provider")]
run-settings-daemon *ARGS:
    cargo run -p glimpse-settings-daemon -- "$@"

[doc("run the screenshot tool: capture a screen or a region headless, or list outputs and windows")]
run-shot *ARGS:
    cargo run -p glimpse-shot -- "$@"

[doc("run the CLI")]
ctl *ARGS:
    cargo run -q -p glimpsectl -- "$@"

[doc("network test safety net: baseline | restore | arm [s] | disarm | status")]
net-guard *ARGS:
    scripts/net-guard.sh "$@"

[doc("click a point on a niri output with a wlr virtual pointer")]
click *ARGS:
    python3 scripts/click.py "$@"

[doc("render one blueprint with the real widgets; reloads on save")]
preview BLUEPRINT *ARGS:
    case "{{ BLUEPRINT }}" in \
    	*crates/glimpse-agents/*) \
    		cargo run -q -p glimpse-agents --example agents-preview -- "{{ BLUEPRINT }}" {{ ARGS }} ;; \
    	*) \
    		cargo run -q -p glimpse-widgets --features preview --example preview -- "{{ BLUEPRINT }}" {{ ARGS }} ;; \
    esac

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

[doc("PSS, swap, anon, threads and CPU of running glimpse processes")]
perf-baseline *ARGS:
    scripts/perf-baseline.sh "$@"

[doc("build only the shipped binaries, release — unlike build-release, doesn't need every workspace crate to compile")]
build-release-binaries:
    GLIMPSE_BINARIES="{{ binaries }}" scripts/build-binaries.sh --release

# ---------------------------------------------------------------- i18n

languages := replace(trim(read("po/LINGUAS")), "\n", " ")

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

# ---------------------------------------------------------------- docs

[doc("install the documentation site's npm dependencies")]
[working-directory("website")]
docs-install:
    npm ci

[doc("serve the documentation site with live reload")]
[working-directory("website")]
docs-dev:
    npm run dev

[doc("regenerate the command line reference in website/config/cli/ from each binary's --help")]
docs-gen-cli:
    GLIMPSE_DOCS_CLI="glimpsectl glimpse-shot glimpse-lens glimpse-lock" scripts/docs-gen-cli.sh

[doc("capture the documentation screenshots in a sealed sandbox: nested niri, private buses, mocked services")]
docs-screenshots: docs-sandbox-build
    scripts/docs-screenshots.py

[doc("record the home page demo video in the same sandbox into website/public/video; --dark records the dark variant")]
docs-video *ARGS: docs-sandbox-build
    scripts/docs-screenshots.py --video {{ ARGS }}

[doc("record the release video in the same sandbox into var/release-video; --reuse renders the last recording again")]
release-video *ARGS: docs-sandbox-build
    cargo build -q --release -p glimpse-shot
    target/debug/glimpsectl idle inhibit -- scripts/release-video.py {{ ARGS }}

[private]
docs-sandbox-build: (build-crate "glimpse-panel") (build-crate "glimpsectl") (build-crate "glimpse-notifications") (build-crate "glimpse-weather") (build-crate "glimpse-wallpaper") (build-crate "glimpse-idle") (build-crate "glimpse-settings-daemon") (build-crate "glimpse-lens") (build-crate "glimpse-shot") fake-tray-build click-build

[private]
fake-tray-build:
    cargo build -q -p glimpse-dbus --features testing --example fake-tray

[private]
click-build:
    cargo build -q -p glimpse-wayland --example click

[doc("lint and build the documentation site into website/.vitepress/dist")]
[working-directory("website")]
docs-build:
    npm run build

# ---------------------------------------------------------------- package

[doc("fail unless TAG (e.g. v0.16.0) matches workspace.package.version in Cargo.toml")]
release-verify TAG:
    scripts/release-verify.sh "$@"

[doc("build a release tarball (glimpse-<version>-<arch>.tar.zst) under dist/ — builds its own binaries")]
package-binary VERSION="": build-translations
    GLIMPSE_BINARIES="{{ binaries }}" scripts/package-binary.sh {{ quote(VERSION) }}

[doc("build a .deb under target/debian/ (needs: cargo install cargo-deb)")]
package-deb: build-release-binaries build-translations
    cargo deb -p glimpse-package --no-build

[doc("build the Fedora .rpm under target/generate-rpm/ (needs: cargo install cargo-generate-rpm)")]
package-rpm-fedora: build-release-binaries build-translations
    cargo generate-rpm -p crates/glimpse-package

[doc("build the openSUSE .rpm under target/generate-rpm/ (needs: cargo install cargo-generate-rpm)")]
package-rpm-opensuse: build-release-binaries build-translations
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
install: build-release-binaries build-translations
    GLIMPSE_BINARIES="{{ binaries }}" scripts/install.sh

[doc("remove installed files")]
uninstall:
    GLIMPSE_BINARIES="{{ binaries }}" scripts/uninstall.sh

[doc("build the Arch package and install it with pacman (elevate with $GLIMPSE_SUDO, default sudo)")]
install-aur: package-aur
    scripts/install-aur.sh

[doc("remove the installed Arch package (elevate with $GLIMPSE_SUDO, default sudo)")]
uninstall-aur:
    scripts/uninstall-aur.sh

[doc("install the agent skills listed in apm.yml into .claude/skills and .agents/skills")]
install-skills:
    uvx --from apm-cli==0.34.0 apm install --target claude,codex
