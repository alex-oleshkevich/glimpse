# Installation

Glimpse is currently easiest to install on Arch-based systems from the AUR.

## Install

```sh
yay -S glimpse-desktop-bin
```

Use another AUR helper if you prefer one.

## Enable the desktop pieces

For a normal Niri setup, enable the shell and lock screen:

```sh
systemctl --user enable --now glimpse-shell.service
systemctl --user enable --now glimpse-lock.service
```

This starts the panel and the background services Glimpse uses for locking and idle behavior. Idle policy and night light run inside the shell service.

## Check it worked

You should see the Glimpse panel in your Niri session.

If it does not appear, check the shell service:

```sh
systemctl --user status glimpse-shell.service
```

For logs:

```sh
journalctl --user -u glimpse-shell.service -e
```

## Customize it

Next, create your config file, choose what appears in the panel, and tune background services. Start with [Configuration](./configuration.md).

| Goal | Page |
|---|---|
| Set up panels and applets | [Configuration](./configuration.md) |
| Tune night light and idle rules | [Configuration](./configuration.md#services) |
| Change colors and CSS | [Theming](./theming.md) |
| Configure wallpaper | [Wallpaper](./wallpaper.md) |
| Configure lock screen | [Lock](./lock.md) |
| Add custom widgets | [Custom Applets](./custom-applets/) |

## Common first fixes

| Problem | Try this |
|---|---|
| Panel does not appear | Make sure you are inside a Niri session, then check `glimpse-shell.service`. |
| Wallpaper does not show | Stop other wallpaper tools first. |
| Lock does nothing | Enable `glimpse-lock.service`, then run `loginctl lock-session`. |
| Night light does not change color | Check [Night Light](./configuration.md#night-light). |
| Idle rules do nothing | Check [Idle](./configuration.md#idle). |
