# Set a light and dark wallpaper

The wallpaper lives under `[wallpaper]`. `image` is the picture for light mode and `image-dark` the
one shown while the desktop is dark; without `image-dark`, `image` shows all the time. The full
setting list is in the [wallpaper reference](/config/services/wallpaper).

```toml
[wallpaper]
image = "/home/alex/Pictures/dunes-day.jpg"
image-dark = "/home/alex/Pictures/dunes-night.jpg"
```

`fit` decides how a picture that does not match the screen is drawn: `cover` (the default),
`contain`, `fill` or `center`. `color` paints underneath whatever the picture leaves open.

## A different picture per monitor

`[[wallpaper.outputs]]` overrides the main table for one monitor, named by its connector. A field
you leave out falls back to `[wallpaper]`:

```toml
[[wallpaper.outputs]]
monitor = "DP-1"
image = "/home/alex/Pictures/garden-day.jpg"
image-dark = "/home/alex/Pictures/garden-night.jpg"
```

`glimpsectl panel applets` lists the connector names if you are unsure of yours.
