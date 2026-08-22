# glimpse task runner. This is the entry point for every workflow —
# do not invoke cargo directly, add or fix a recipe here instead.

set shell := ["bash", "-uc"]

prefix := env("PREFIX", "/usr")
destdir := env("DESTDIR", "")
bindir := destdir / prefix / "bin"
unitdir := destdir / prefix / "lib/systemd/user"
dbusdir := destdir / prefix / "share/dbus-1/services"
pamdir := destdir / "/etc/pam.d"
sharedir := destdir / prefix / "share/glimpse"

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

[doc("clippy, warnings are errors")]
lint:
    cargo clippy --workspace --all-targets -- -D warnings

[doc("clippy on one crate")]
lint-crate CRATE:
    cargo clippy -p {{ CRATE }} --all-targets -- -D warnings

[doc("format in place")]
fmt:
    cargo fmt --all

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
    systemd-analyze --user verify data/systemd/*.service

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
    install -Dm755 target/release/glimpsed          {{ bindir }}/glimpsed
    install -Dm755 target/release/glimpse           {{ bindir }}/glimpse
    install -Dm755 target/release/glimpse-wallpaper {{ bindir }}/glimpse-wallpaper
    install -Dm755 target/release/glimpse-lock      {{ bindir }}/glimpse-lock
    install -Dm755 target/release/glimpsectl        {{ bindir }}/glimpsectl
    for f in data/systemd/*.service; do [ -e "$f" ] && install -Dm644 "$f" {{ unitdir }}/"$(basename $f)"; done
    for f in data/dbus-1/services/*.service; do [ -e "$f" ] && install -Dm644 "$f" {{ dbusdir }}/"$(basename $f)"; done
    for f in data/pam.d/*; do [ -e "$f" ] && [ "$(basename $f)" != .gitkeep ] && install -Dm644 "$f" {{ pamdir }}/"$(basename $f)"; done
    install -Dm644 data/config.default.toml {{ sharedir }}/config.default.toml
    install -Dm644 data/config.schema.json {{ sharedir }}/config.schema.json

[doc("remove installed files")]
uninstall:
    rm -f {{ bindir }}/{glimpsed,glimpse,glimpse-wallpaper,glimpse-lock,glimpsectl}
    rm -f {{ unitdir }}/glimpse*.service {{ dbusdir }}/org.kde.StatusNotifierWatcher.service
    rm -f {{ dbusdir }}/org.freedesktop.Notifications.service {{ pamdir }}/glimpse-lock
    rm -f {{ sharedir }}/config.default.toml {{ sharedir }}/config.schema.json
