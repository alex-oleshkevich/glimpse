# Wallpaper

Wallpaper and backdrop settings live in `~/.config/glimpse/config.toml`.

## Image wallpaper

Use an absolute path for the image you want on the desktop:

```toml
[wallpaper]
path = "/home/alex/Pictures/wallpapers/coast.jpg"
color = "#101010"
fit = "cover"
transition_ms = 800

[backdrop]
enabled = true
blur_radius = 24
```

`color` is the fallback behind the image. Keep it close to the wallpaper's dominant tone so startup, loading failures, and transparent edges still look intentional.

## Solid color

Leave `path` out when you want a plain background:

```toml
[wallpaper]
color = "#101010"
fit = "cover"
transition_ms = 800

[backdrop]
enabled = false
```

This is useful for minimal setups or screenshots where the panel should be the focus.

## Fit

| Mode | Result |
|---|---|
| `cover` | Fills the screen and crops edges when needed. Best default for screenshots. |
| `contain` | Shows the whole image and leaves empty space if the aspect ratio differs. |
| `fill` | Stretches the image to the screen. Use only when distortion is acceptable. |

## Backdrop

The backdrop is a softer image layer used behind translucent surfaces. By default it is enabled and uses the wallpaper image.

Use a separate backdrop image when the wallpaper is too busy behind panels or popovers:

```toml
[backdrop]
enabled = true
path = "/home/alex/Pictures/wallpapers/coast-blurred.jpg"
blur_radius = 24
```

Set `enabled = false` if you want no backdrop layer.

## What wins

Explicit config paths win over theme-pack images.

| Surface | Source order |
|---|---|
| Wallpaper | `[wallpaper].path`, then the active theme pack's wallpaper image, then `color` |
| Backdrop | `[backdrop].path`, then the active theme pack's backdrop image, then the resolved wallpaper image |

Theme-pack images can provide light and dark variants. The current `theme_mode` decides which variant is used. See [Theming](./theming.md#theme-packs) for theme-pack filenames.

## Reloading

Glimpse watches the config and the currently used image files. Changing `config.toml` or replacing the current image updates the background without a restart.

If a new image cannot be loaded, the previous image stays visible when possible. If no image is available, Glimpse falls back to `color`.

## Practical tips

| Goal | Tip |
|---|---|
| Clean screenshot | Use `cover`, a high-resolution image, and a dark fallback color. |
| Calm popovers | Keep backdrop enabled and use `blur_radius = 24` or higher. |
| Faster iteration | Replace the image at the same path instead of renaming files every time. |
| Multi-monitor | Choose images that still look good when cropped differently per output. |

### See also

| Document | Purpose |
|---|---|
| [Lock Screen](./lock.md) | Use the same wallpaper or a separate lock background. |
| [Theming](./theming.md) | Theme-pack image names and light/dark variants. |
