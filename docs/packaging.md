# Packaging guide

Use this guide when maintaining a distribution package, release archive, or local system install for Glimpse. It documents the files shipped by the current binary package and the service behavior expected by the user-facing installation docs.

Most users should install `glimpse-desktop-bin` and follow [Installation](./installation.md). This page is for packagers and maintainers.

## Package contents

The binary release archive is produced by `scripts/package-binary.sh`. It installs this runtime tree:

```text
etc/geoclue/conf.d/glimpse.conf
etc/pam.d/glimpse-lock
usr/bin/glimpse-lock
usr/bin/glimpse-shell
usr/bin/glimpse-wallpaper
usr/lib/systemd/user/glimpse-lock.service
usr/lib/systemd/user/glimpse-shell.service
usr/lib/systemd/user/glimpse-wallpaper.service
usr/share/dbus-1/services/me.aresa.GlimpseIdle.Portal.service
usr/share/glimpse/applets/<name>/applet.toml
usr/share/glimpse/applets/<name>/<binary>
usr/share/glimpse/applet-templates/<language>/...
usr/share/glimpse/scripts/monitors
usr/share/glimpse/themes/<name>/...
usr/share/xdg-desktop-portal/portals/glimpse.portal
```

The archive may also include `LICENSE` at the archive root. A distro package should install that into its normal license location.

## Binaries

| Binary | Purpose | Install to |
|---|---|---|
| `glimpse-shell` | GTK4 layer-shell panel, applet host, idle policy, idle portal backend, and night light service | `/usr/bin/glimpse-shell` |
| `glimpse-lock` | Wayland session lock screen and logind lock listener | `/usr/bin/glimpse-lock` |
| `glimpse-wallpaper` | Wallpaper and backdrop daemon | `/usr/bin/glimpse-wallpaper` |

Build all shipped binaries together:

```bash
cargo build --release \
  -p glimpse-lock \
  -p glimpse-shell \
  -p glimpse-wallpaper
```

After building a release archive, verify each binary reports the release version:

```bash
target/release/glimpse-lock --version
target/release/glimpse-shell --version
target/release/glimpse-wallpaper --version
```

## Runtime dependencies

The Arch binary package currently depends on GTK4, libadwaita, gtk4-layer-shell, libheif, PAM, and GeoClue.

Packagers should also preserve access to the session D-Bus bus, Wayland socket, fontconfig, icon themes, and GPU/image decoding paths. The shipped systemd units intentionally avoid sandboxing that would break those desktop integrations.

## Systemd user units

Install the unit files from `data/` without rewriting them:

| Source | Install to |
|---|---|
| `data/glimpse-shell.service` | `/usr/lib/systemd/user/glimpse-shell.service` |
| `data/glimpse-wallpaper.service` | `/usr/lib/systemd/user/glimpse-wallpaper.service` |
| `data/glimpse-lock.service` | `/usr/lib/systemd/user/glimpse-lock.service` |

All shipped units are user services and are bound to `graphical-session.target` with `PartOf=` and `Requisite=`. They use `Type=exec`, restart on failure except for the lock screen, and include conservative hardening fields where compatible with the daemon.

| Unit | Important behavior |
|---|---|
| `glimpse-shell.service` | Wants `glimpse-wallpaper.service`; starts after `graphical-session.target` and `glimpse-wallpaper.service`. |
| `glimpse-wallpaper.service` | Starts after `graphical-session.target`; needs normal desktop/GPU access for image handling. |
| `glimpse-lock.service` | Uses `Restart=always` so a clean exit while locked is treated as something to respawn. |

User-facing install docs ask users to enable:

```bash
systemctl --user enable --now glimpse-shell.service
systemctl --user enable --now glimpse-lock.service
```

Idle policy, the idle inhibitor portal, and night light run inside `glimpse-shell.service`. `glimpse-wallpaper.service` starts through `glimpse-shell.service` but can also be enabled explicitly if a session wants wallpaper before the panel.

## PAM

`glimpse-lock` authenticates through PAM service `glimpse-lock`. Install the shipped file:

| Source | Install to |
|---|---|
| `data/pam.d/glimpse-lock` | `/etc/pam.d/glimpse-lock` |

The file includes the system `auth` and `account` stacks and denies `password` and `session` phases. That matches the lock screen's current PAM usage: authenticate and check account status only.

## GeoClue

Install the GeoClue config so the shell can request location for weather, automatic theme mode, and automatic night light:

| Source | Install to |
|---|---|
| `data/geoclue/glimpse.conf` | `/etc/geoclue/conf.d/glimpse.conf` |

The file grants the `glimpse-shell` desktop id access through GeoClue. Shared location config currently supports GeoClue only; do not document static shared coordinates as a packaged feature.

## Idle portal

The shell ships an xdg-desktop-portal backend for inhibit requests:

| Source | Install to |
|---|---|
| `data/portals/glimpse.portal` | `/usr/share/xdg-desktop-portal/portals/glimpse.portal` |
| `data/dbus-1/me.aresa.GlimpseIdle.Portal.service` | `/usr/share/dbus-1/services/me.aresa.GlimpseIdle.Portal.service` |

The portal descriptor exposes `org.freedesktop.impl.portal.Inhibit` for Glimpse sessions. The D-Bus service activates `glimpse-shell.service` through systemd.

## Monitor helper

Install the monitor helper used by the default idle config:

| Source | Install to |
|---|---|
| `data/scripts/monitors` | `/usr/share/glimpse/scripts/monitors` |

It supports:

| Compositor | Detection | Command |
|---|---|---|
| niri | `NIRI_SOCKET` | `niri msg action power-on-monitors` / `power-off-monitors` |
| Hyprland | `HYPRLAND_INSTANCE_SIGNATURE` | `hyprctl dispatch dpms on` / `off` |

If a package moves this helper, the default `[idle]` commands in the shared config must change too.

## Themes, applets, and templates

Install packaged theme packs under `/usr/share/glimpse/themes/<name>/`. The current binary package ships the `rosepine` pack when present in `themes/rosepine`.

Install bundled applets under `/usr/share/glimpse/applets/<name>/` with each applet's `applet.toml` and executable. The build helper is:

```bash
scripts/build-glimpse-applets.sh glimpse-applets "$pkgdir/usr/share/glimpse/applets"
```

Install applet project templates under `/usr/share/glimpse/applet-templates/`. `glimpse-shell applets new` reads templates from disk, so missing templates break applet scaffolding even when the shell binary works.

## Configuration files

| Config | Discovery or location |
|---|---|
| Shared desktop config | `GLIMPSE_CONFIG`, `./config.toml`, `$XDG_CONFIG_HOME/glimpse/config.toml`, then `$HOME/.config/glimpse/config.toml` when `XDG_CONFIG_HOME` is unset |
| User theme packs | `$XDG_CONFIG_HOME/glimpse/themes/<name>/` |
| System theme packs | `/usr/share/glimpse/themes/<name>/` |
| User applet packages | `$XDG_CONFIG_HOME/glimpse/applets/*.toml` or linked project applets |
| System applet packages | `/usr/share/glimpse/applets/<name>/applet.toml` |

Do not duplicate full user config examples in packaging docs. Link to [Configuration](./configuration.md) and [Services](./configuration.md#services) so packager docs do not drift from the config reference.

## Manual install commands

These commands mirror the release packaging script for the core files:

```bash
install -Dm755 target/release/glimpse-lock "$pkgdir/usr/bin/glimpse-lock"
install -Dm755 target/release/glimpse-shell "$pkgdir/usr/bin/glimpse-shell"
install -Dm755 target/release/glimpse-wallpaper "$pkgdir/usr/bin/glimpse-wallpaper"
install -Dm755 data/scripts/monitors "$pkgdir/usr/share/glimpse/scripts/monitors"

install -Dm644 data/glimpse-lock.service "$pkgdir/usr/lib/systemd/user/glimpse-lock.service"
install -Dm644 data/glimpse-shell.service "$pkgdir/usr/lib/systemd/user/glimpse-shell.service"
install -Dm644 data/glimpse-wallpaper.service "$pkgdir/usr/lib/systemd/user/glimpse-wallpaper.service"

install -Dm644 data/pam.d/glimpse-lock "$pkgdir/etc/pam.d/glimpse-lock"
install -Dm644 data/geoclue/glimpse.conf "$pkgdir/etc/geoclue/conf.d/glimpse.conf"
install -Dm644 data/portals/glimpse.portal "$pkgdir/usr/share/xdg-desktop-portal/portals/glimpse.portal"
install -Dm644 data/dbus-1/me.aresa.GlimpseIdle.Portal.service "$pkgdir/usr/share/dbus-1/services/me.aresa.GlimpseIdle.Portal.service"
```

No polkit policy or battery helper is shipped in the current package tree.

## Release archive notes

The AUR package `glimpse-desktop-bin` consumes GitHub release archives named:

```text
glimpse-<version>-x86_64.tar.zst
```

The local release helper builds the archive and writes a BLAKE2 checksum:

```bash
scripts/package-binary.sh <version>
```

The source `PKGBUILD` keeps `b2sums_x86_64=('SKIP')` as a template. The release process renders the AUR copy with the checksum for the uploaded archive.

## See also

| Page | Use it for |
|---|---|
| [Installation](./installation.md) | User-facing package install and service enablement. |
| [Configuration](./configuration.md) | Shared config file discovery, services, and top-level settings. |
| [Theming](./theming.md) | Theme pack search paths and CSS token contracts. |
| [IPC Developer Specification](./ipc.md) | Adding daemon IPC commands and events. |
