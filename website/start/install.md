# Install glimpse

glimpse needs GTK 4.22 and libadwaita 1.9 or newer. That rules out older distribution releases; the
packages below target the first releases that ship both.

## Install the package

::: code-group

```sh [Arch Linux]
yay -S glimpse-desktop-bin
```

```sh [Debian, Ubuntu, Fedora, openSUSE]
curl -fsSL https://raw.githubusercontent.com/alex-oleshkevich/glimpse/master/install.sh | bash
```

```nix [NixOS]
{
  inputs.glimpse.url = "github:alex-oleshkevich/glimpse";
  outputs = { nixpkgs, glimpse, ... }: {
    nixosConfigurations.host = nixpkgs.lib.nixosSystem {
      modules = [ glimpse.nixosModules.default { services.glimpse.enable = true; } ];
    };
  };
}
```

```sh [From source]
git clone https://github.com/alex-oleshkevich/glimpse
cd glimpse
just build-release-binaries build-translations
sudo GLIMPSE_BINARIES="$(just --evaluate binaries)" scripts/install.sh
```

:::

### Arch Linux

`glimpse-desktop-bin` is in the AUR. Use your AUR helper, or build it with `makepkg`. Some applets
and features need extra packages:

| Install | For |
| --- | --- |
| `networkmanager` | the network applet |
| `bluez` | the Bluetooth applet |
| `upower` | battery level and the charge of connected devices |
| `power-profiles-daemon` | power modes in the battery popover |
| `udisks2` | the removable media applet |
| `cups` | the printing applet |
| `kdeconnect` | the phone applet |
| `ddcutil` | brightness control for external monitors |
| `xdg-desktop-portal` | letting apps hold off the idle timers |
| `xdg-desktop-portal`, a settings backend such as `xdg-desktop-portal-gtk`, and `dconf` | switching the desktop between light and dark mode |

### Debian, Ubuntu, Fedora and openSUSE

The packages target Debian 14 (forky, still in testing), Ubuntu 26.04, Fedora 44 and openSUSE
Tumbleweed. The install script downloads the latest release as a `.deb` or `.rpm`, checks it against
the published checksums and installs it with your package manager, so it asks for your `sudo`
password. On an older release, such as Debian 13, Ubuntu 25.10, Fedora 43 or openSUSE Leap, it stops
with an error instead.

On other distributions the script unpacks the binary tarball into `/` as root, asking for `sudo`.
It preserves existing PAM, GeoClue and portal configuration files, including symlinks, except for
the unchanged published 0.15 GeoClue default, which it replaces so the new client has permission.
For other existing files, a differing packaged default is saved beside the original with a `.new`
suffix and a review message. The old `glimpse-shell` binary, its user service and idle portal
activation file are removed only when absent from the selected archive. Custom themes
and SDK files are left alone. The `.deb` and `.rpm`
packages are the supported route. On Arch Linux the script prints the AUR command without
installing; on NixOS it refuses and points you to the flake module.

Every package and tarball on the releases page carries a build attestation from the release
workflow. To check that a file you downloaded was built by this repository from a release tag:

```sh
gh attestation verify glimpse-<version>-x86_64.tar.zst --repo alex-oleshkevich/glimpse
```

If you would rather not pipe a script into a shell, download the package for your distribution from
the [releases page](https://github.com/alex-oleshkevich/glimpse/releases) and install it yourself.

### NixOS

Add the flake input and enable the module as shown above. The module installs the package, its
systemd units and D-Bus services, the PAM entry for the lock screen, and a GeoClue policy for the
night light and weather. Rebuild with `nixos-rebuild switch`. The module does not enable
`glimpse-session.target`, so start the target from your compositor's startup commands as described
below instead of enabling it.

### From source

You need Rust 1.95 or newer, [`just`](https://github.com/casey/just), `blueprint-compiler`, gettext
and clang, plus the development files for GTK4, libadwaita, gtk4-layer-shell, PulseAudio, PipeWire,
PAM and udev. The install copies files under `/usr` and `/etc`, so it needs root.
[Build glimpse from source](/contribute/build) lists the packages for Debian, Ubuntu and Arch and the exact
commands.

## Start the shell

glimpse runs as systemd user services grouped under `glimpse-session.target`, which starts and stops
with `graphical-session.target`. An explicit stop uses `--job-mode=replace-irreversibly` so a
concurrent app request cannot cancel it. The lock screen stays with the graphical session; stopping
the shell target does not stop it.

If your session starts `graphical-session.target`, as a uwsm session does, enable the target once
from a terminal inside your Wayland session. It then starts and stops with every graphical session:

```sh
systemctl --user enable --now glimpse-session.target
```

If your session does not start `graphical-session.target`, or you are on NixOS, start the target
when the compositor starts and stop it when the compositor exits. Otherwise it outlives the
compositor while your user manager runs:

::: code-group

```lua [hyprland.lua]
hl.on("hyprland.start", function()
    hl.exec_cmd("systemctl --user start glimpse-session.target")
end)
hl.on("hyprland.shutdown", function()
    hl.exec_cmd("systemctl --user --job-mode=replace-irreversibly stop glimpse-session.target")
end)
```

```ini [hyprland.conf]
exec-once = systemctl --user start glimpse-session.target
exec-shutdown = systemctl --user --job-mode=replace-irreversibly stop glimpse-session.target
```

:::

The panel should appear within a second or two.

## Check the install

```sh
glimpsectl doctor
```

`doctor` checks each part of glimpse and the services it depends on, and tells you what is missing.
It exits with status 1 if anything is wrong.

Continue with [your first session](./first-session).
