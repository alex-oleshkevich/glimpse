# glimpse

A desktop shell for Wayland: a panel, a wallpaper and a lock screen, built with GTK4 and
libadwaita.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/shell-dark.png">
  <img alt="The glimpse panel over the default wallpaper" src="docs/screenshots/shell-light.png">
</picture>

## About

glimpse adds what a bare Wayland compositor leaves out: a bar, notifications, a wallpaper, a lock
screen, idle timers and a night light. It follows your light or dark theme and accent color, and
blurs what sits behind it.

## Demo

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/demo-dark.gif">
  <img alt="glimpse in use" src="docs/screenshots/demo.gif">
</picture>

Watch it in full quality: [light](website/public/video/demo.mp4), [dark](website/public/video/demo-dark.mp4).

## Features

- Quick popovers for Wi-Fi, Bluetooth, audio, the calendar and the weather, and a few more.
- Notifications keep a history, and do not disturb holds them back.
- The lock screen authenticates through PAM.
- Idle timers blank the screens, lock and suspend, and can differ between AC and battery.
- The night light follows sunset and sunrise where you are.
- Exec applets put your own program on the panel, with its own indicator and popover. SDKs exist
  for TypeScript, Python, Go and Rust.
- Settings live in `config.toml` and the panel layout in `panel.toml`. Saving either applies the
  change, and a JSON schema gives your editor completion.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/applets-dark.png">
  <img alt="Calendar, weather, Bluetooth and network popovers" src="docs/screenshots/applets-light.png">
</picture>

## Installation

On Arch Linux, install the package from the AUR:

```sh
yay -S glimpse-desktop-bin
```

On Debian 14 (forky, still in testing), Ubuntu 26.04, Fedora 44 or openSUSE Tumbleweed and newer, the
install script downloads the latest release as a `.deb` or `.rpm`, checks it against the published
checksums and installs it (older releases lack GTK 4.22 and libadwaita 1.9, so it refuses them):

```sh
curl -fsSL https://raw.githubusercontent.com/alex-oleshkevich/glimpse/master/install.sh | bash
```

On NixOS, import the flake's module and start `glimpse-session.target` from your compositor, as
under [Start the shell](#start-the-shell):

```nix
{
  inputs.glimpse.url = "github:alex-oleshkevich/glimpse";
  outputs = { nixpkgs, glimpse, ... }: {
    nixosConfigurations.host = nixpkgs.lib.nixosSystem {
      modules = [ glimpse.nixosModules.default { services.glimpse.enable = true; } ];
    };
  };
}
```

To build from source you need Rust 1.95 or newer, `just`, `blueprint-compiler`, gettext and clang,
plus the development files for GTK4, libadwaita, gtk4-layer-shell, PulseAudio, PipeWire, PAM and udev. Then:

```sh
just build-release-binaries build-translations
sudo GLIMPSE_BINARIES="$(just --evaluate binaries)" scripts/install.sh
```

### Start the shell

glimpse runs as systemd user services under `glimpse-session.target`, which follows
`graphical-session.target`. Explicit shell stops use `--job-mode=replace-irreversibly` so concurrent
D-Bus activation cannot cancel them; the lock service stays with the graphical session. If your
session starts `graphical-session.target` (niri's `niri-session`, uwsm), enable the shell target once:

```sh
systemctl --user enable --now glimpse-session.target
```

Otherwise (plain Hyprland, NixOS) start it with the compositor and stop it on exit. In `hyprland.lua`:

```lua
hl.on("hyprland.start", function()
    hl.exec_cmd("systemctl --user start glimpse-session.target")
end)
hl.on("hyprland.shutdown", function()
    hl.exec_cmd("systemctl --user --job-mode=replace-irreversibly stop glimpse-session.target")
end)
```

In a legacy `hyprland.conf`:

```ini
exec-once = systemctl --user start glimpse-session.target
exec-shutdown = systemctl --user --job-mode=replace-irreversibly stop glimpse-session.target
```

The configuration lives in `~/.config/glimpse/`: `config.toml` for settings and `panel.toml` for the
panel layout. Every setting and its default is listed, with comments, in
[`data/config.commented.toml`](data/config.commented.toml) and
[`data/panel.commented.toml`](data/panel.commented.toml).

## Credits

glimpse is built on [GTK4](https://gtk.org), [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/),
[gtk4-layer-shell](https://github.com/wmww/gtk4-layer-shell), [relm4](https://relm4.org),
[zbus](https://github.com/dbus2/zbus) and [tokio](https://tokio.rs). Weather and geocoding come from
[Open-Meteo](https://open-meteo.com), location from [GeoClue](https://gitlab.freedesktop.org/geoclue/geoclue),
and icons from the [Adwaita](https://gitlab.gnome.org/GNOME/adwaita-icon-theme) icon theme.

glimpse is released under the [BSD 3-Clause license](LICENSE).
