# Lock screen

The lock screen uses the shared config file at `~/.config/glimpse/config.toml`. It can reuse your wallpaper, use a separate lock image, show a clock, and show status buttons.

## Enable locking

Enable the user service:

```sh
systemctl --user enable --now glimpse-lock.service
```

Then test it:

```sh
loginctl lock-session
```

Use `loginctl lock-session` from compositor keybindings and idle rules. Keep the service running so locking happens immediately.

## Use your wallpaper

Leave `[lock.background].path` unset to use the same image as `[wallpaper]`:

```toml
[wallpaper]
path = "/home/alex/Pictures/wallpapers/coast.jpg"
color = "#101010"
fit = "cover"

[lock]
css_path = "themes/lock.css"

[lock.background]
blur_radius = 0
dim = 0.35
```

This is the best starting point when you want the lock screen to match the desktop.

## Use a separate lock image

Set `[lock.background].path` when the lock screen needs its own image:

```toml
[lock.background]
path = "/home/alex/Pictures/wallpapers/lock.jpg"
color = "#101010"
fit = "cover"
blur_radius = 12
dim = 0.35
```

`blur_radius` softens the background. `dim` darkens it; `0.0` means no dimming and `1.0` means fully dimmed. `color` is an optional fallback shown behind the image (or alone if the image fails to load); when unset it falls back to `[wallpaper].color`.

If `fit` is left out, a lock-specific image uses `cover`. When the lock screen inherits the wallpaper image, it also inherits the wallpaper fit mode.

## Background fallback

Config paths win over theme-pack images.

| Source order | Used when |
|---|---|
| `[lock.background].path` | You set a lock-specific image. |
| `[wallpaper].path` | No lock-specific image is set. |
| Theme pack `lock-light` / `lock-dark` image | Config has no lock or wallpaper image. |
| Theme pack `wallpaper-light` / `wallpaper-dark` image | The theme has no lock-specific image. |
| `[lock.background].color` | You set a lock-specific fallback color. |
| `[wallpaper].color` | No lock-specific color is set. Shown behind the image, or alone when no image is available. |

The current `theme_mode` decides whether light or dark theme-pack images are used.

## Clock

```toml
[lock.clock]
enabled = true
time_format = "%H:%M"
date_format = "%A, %B %-d"
```

Set `enabled = false` if you want the lock screen to be image-first with no large clock.

## Status buttons

```toml
[lock.controls]
buttons = ["wifi", "input", "weather", "battery", "power"]
```

| Button | What it shows |
|---|---|
| `wifi` | Network status. |
| `input` | Current keyboard layout. |
| `weather` | Weather icon and temperature. |
| `battery` | Battery status. Percent is shown when running on battery. |
| `power` | Suspend, restart, and shutdown menu. |

Remove entries you do not want:

```toml
[lock.controls]
buttons = ["wifi", "battery", "power"]
```

## CSS

The default lock CSS override path is:

```txt
~/.config/glimpse/themes/lock.css
```

Keep small lock-screen visual tweaks there. See [Theming](./theming.md#lock-css) for lock CSS variables.

## Practical tips

| Goal | Tip |
|---|---|
| Match desktop and lock screen | Leave `[lock.background].path` unset. |
| Make text easier to read | Increase `dim` or add a small `blur_radius`. |
| Use a busy wallpaper | Use a separate, calmer `[lock.background].path`. |
| Avoid accidental shutdown | Keep the `power` button only if you want power actions on the lock screen. |

### See also

| Document | Purpose |
|---|---|
| [Wallpaper](./wallpaper.md) | Desktop wallpaper, backdrop, and fit modes. |
| [Theming](./theming.md#lock-css) | Lock CSS variables and override file. |
