# Build glimpse from source

## Install the dependencies

You need Rust 1.95 or newer, [`just`](https://github.com/casey/just), `blueprint-compiler`,
gettext, clang, ripgrep and python3, plus the development files for GTK4, libadwaita,
gtk4-layer-shell, PulseAudio, PipeWire, PAM and udev.

::: code-group

```sh [Debian or Ubuntu]
sudo apt-get install libgtk-4-dev libadwaita-1-dev libgtk4-layer-shell-dev libheif-dev \
    libpam0g-dev libglib2.0-dev libclang-dev libpulse-dev libpipewire-0.3-dev libudev-dev \
    libsystemd-dev pkg-config blueprint-compiler desktop-file-utils gettext ripgrep python3
```

```sh [Arch]
sudo pacman -S --needed base-devel rust just blueprint-compiler gettext clang ripgrep python \
    desktop-file-utils gtk4 libadwaita gtk4-layer-shell libpulse libpipewire pam
```

:::

The Debian and Ubuntu line is the one CI builds with. It does not include Rust or `just`: install
Rust with [rustup](https://rustup.rs) and `just` from its releases page, because the packaged Rust is
usually older than 1.95. Older releases may also not carry GTK 4.22 and libadwaita 1.9, which glimpse
needs.

## Clone and verify

```sh
git clone https://github.com/alex-oleshkevich/glimpse
cd glimpse
just
```

`just` alone lists every recipe. Run what CI runs before you send a change: formatting, the type
check, clippy with warnings as errors, the tests and the translation checks.

```sh
just verify
```

## Install

Build as yourself, then copy the files as root:

```sh
just build-release-binaries build-translations
sudo GLIMPSE_BINARIES="$(just --evaluate binaries)" scripts/install.sh
```

The second line copies the binaries, systemd units, D-Bus services, the PAM entry for the lock screen
and the data files under `/usr` and `/etc`. Your `config.toml` and `panel.toml` are written the
first time a glimpse program starts.
The units and D-Bus services expect the binaries in `/usr/bin`, so keep the default prefix for a
working desktop. To stage the files for a package instead, set `DESTDIR`:

```sh
DESTDIR=/tmp/glimpse-stage just install
```

Then start the shell once, from a terminal inside your Wayland session:

```sh
systemctl --user enable --now glimpse-session.target
```

[Install glimpse](/start/install) covers starting the shell and checking the install in more detail.
