# Upgrade and uninstall

## Upgrade

Upgrade the way you installed:

::: code-group

```sh [Arch Linux]
yay -Syu glimpse-desktop-bin
```

```sh [Debian, Ubuntu, Fedora, openSUSE]
curl -fsSL https://raw.githubusercontent.com/alex-oleshkevich/glimpse/master/install.sh | bash
```

```sh [NixOS]
nix flake update glimpse
sudo nixos-rebuild switch
```

```sh [From source]
git pull
just build-release-binaries build-translations
sudo GLIMPSE_BINARIES="$(just --evaluate binaries)" scripts/install.sh
```

:::

The install script always fetches the latest release, so running it again is the upgrade.

Then restart the shell so the new binaries take over:

```sh
systemctl --user restart glimpse-session.target
```

The lock screen is left alone, because restarting it while it holds the lock would leave you locked
out. It picks up the new version the next time you log in.

Your `config.toml` is never touched by an upgrade. A new release can add settings; they start at
their defaults until you set them. `glimpsectl config validate` tells you if a setting you use was
renamed or removed.

## Uninstall

Stop the shell and keep it from starting with your next session:

```sh
systemctl --user disable --now glimpse-session.target
```

The lock screen keeps running until you log out.

Then remove the package:

::: code-group

```sh [Arch Linux]
sudo pacman -R glimpse-desktop-bin
```

```sh [Debian, Ubuntu]
sudo apt remove glimpse
```

```sh [Fedora]
sudo dnf remove glimpse
```

```sh [openSUSE]
sudo zypper remove glimpse
```

```sh [From source]
sudo GLIMPSE_BINARIES="$(just --evaluate binaries)" scripts/uninstall.sh
```

:::

On NixOS, remove `services.glimpse.enable = true;` and the flake input, then rebuild.

Uninstalling leaves your settings in `~/.config/glimpse/`, saved state in `~/.local/state/glimpse/`
and the desktop entries of any applets you installed in `~/.local/share/applications/`. Delete
those if you don't plan to come back.

Package removal can also leave edited files at `/etc/pam.d/glimpse-lock`,
`/etc/geoclue/conf.d/glimpse.conf`, `/etc/xdg-desktop-portal/glimpse-portals.conf` and
`/etc/xdg-desktop-portal/niri-portals.conf`. On Arch Linux, these are kept as `.pacsave` files.

The source uninstall script also leaves `/usr/share/icons/hicolor/scalable/apps/me.aresa.Glimpse.svg`
and `/usr/share/icons/hicolor/symbolic/apps/me.aresa.Glimpse-symbolic.svg` behind.
