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
just install
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
just uninstall
```

:::

On NixOS, remove `services.glimpse.enable = true;` and the flake input, then rebuild.

Uninstalling leaves your settings in `~/.config/glimpse/`, saved state in `~/.local/state/glimpse/`
and any applets you installed in `~/.local/share/glimpse/`. Delete those directories if you don't
plan to come back.
