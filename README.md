# glimpse

A desktop shell for the niri Wayland compositor: a panel, a wallpaper and a lock screen, built with
GTK4 and libadwaita.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/shell-dark.png">
  <img alt="The glimpse panel over the default wallpaper" src="docs/screenshots/shell-light.png">
</picture>

## About

glimpse gives a bare compositor the parts a desktop needs: a bar with a clock, system status and
quick settings, a wallpaper, notifications, a lock screen, idle handling and a night light. It
follows your light or dark preference and your accent color, and uses the compositor's blur behind
the panel, popovers and notifications.

There is no central daemon. The panel runs its own services, and notifications, weather, idle and
the night light are small standalone programs, each owning one D-Bus name. If one of them crashes,
the others keep running.

niri is the main target. Hyprland support exists but is second class and untested.

## Features

- A panel whose popovers cover the everyday settings: calendar with your events and a world clock,
  weather, Wi-Fi and VPN, Bluetooth, audio, media players, brightness, battery and removable drives.
- Notifications with a history and do not disturb.
- A lock screen that authenticates through PAM.
- Idle handling that blanks the screens, locks and suspends, with separate timings on AC and on
  battery.
- A night light that follows sunset and sunrise at your location.
- One `config.toml` for everything, reloaded as you save it, with a JSON schema for editor
  completion.
- `glimpsectl`, a command-line tool for scripting and checking the setup.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/applets-dark.png">
  <img alt="Calendar, weather, Bluetooth and network popovers" src="docs/screenshots/applets-light.png">
</picture>

## Installation

On Arch Linux, install the package from the AUR:

```sh
yay -S glimpse-desktop-bin
```

On Debian, Ubuntu or Fedora, the install script downloads the latest release as a `.deb` or `.rpm`,
checks it against the published checksums and installs it:

```sh
curl -fsSL https://raw.githubusercontent.com/alex-oleshkevich/glimpse/master/install.sh | bash
```

To build from source you need Rust 1.93 or newer, `just`, `blueprint-compiler` and gettext, plus the
development files for GTK4, libadwaita, gtk4-layer-shell, PulseAudio, PAM and udev. Then:

```sh
just install
```

### Start the shell

glimpse runs as a set of systemd user services. Enable them once from inside your niri session:

```sh
systemctl --user enable --now glimpse-session.target
```

The configuration lives in `~/.config/glimpse/config.toml`. Every setting and its default is listed,
with comments, in [`data/config.commented.toml`](data/config.commented.toml).

## Credits

glimpse is built on [GTK4](https://gtk.org), [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/),
[gtk4-layer-shell](https://github.com/wmww/gtk4-layer-shell), [relm4](https://relm4.org),
[zbus](https://github.com/dbus2/zbus) and [tokio](https://tokio.rs). Weather and geocoding come from
[Open-Meteo](https://open-meteo.com), location from [GeoClue](https://gitlab.freedesktop.org/geoclue/geoclue),
and icons from the [Adwaita](https://gitlab.gnome.org/GNOME/adwaita-icon-theme) icon theme.

glimpse is released under the [BSD 3-Clause license](LICENSE).
