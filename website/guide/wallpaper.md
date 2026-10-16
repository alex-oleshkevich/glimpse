# Set a light and dark wallpaper

The wallpaper lives under `[wallpaper]`. `image` is the picture for light mode and `image-dark` the
one shown while glimpse is dark; without `image-dark`, `image` shows all the time. Which one shows
follows `[appearance] color-scheme`: `desktop` follows the desktop mode set by `desktop-color-scheme`,
while `light` or `dark` keeps one picture up. With `color-scheme = "light"`, you keep the light
picture even when the desktop is dark. The full setting list is in the
[wallpaper reference](/config/services/wallpaper).

```toml
[wallpaper]
image = "~/Pictures/dunes-day.jpg"
image-dark = "~/Pictures/dunes-night.jpg"
```

Without any `image`, glimpse shows its built-in picture. A path can start with `~/`; a relative
path is looked up in `~/.config/glimpse/`.

`fit` decides how a picture that does not match the screen is drawn: `cover` (the default),
`contain`, `fill` or `center`. `color` paints underneath whatever the picture leaves open, and is
also what you see when the image file is missing.

## A different picture per monitor

`[[wallpaper.outputs]]` overrides the main table for one monitor, named by its connector. A field
you leave out falls back to `[wallpaper]`, one by one, so set `image` and `image-dark` together:
an entry with only `image` still takes the dark picture from `[wallpaper]`.

```toml
[[wallpaper.outputs]]
monitor = "DP-1"
image = "~/Pictures/garden-day.jpg"
image-dark = "~/Pictures/garden-night.jpg"
```

An entry for a monitor that is not connected is ignored. `glimpsectl applet list` shows the
connector names of monitors with a bar, and your compositor lists them all (for example
`niri msg outputs`).

## If the picture does not change

glimpse keeps the previous picture when a file is not an image it can read, and paints `color` when
the file does not exist. Both cases are only logged, and `glimpsectl config validate` does not
catch them, so check the path first.
