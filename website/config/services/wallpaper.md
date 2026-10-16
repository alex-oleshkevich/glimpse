# Wallpaper

`[wallpaper]` sets the desktop background. An image can differ between light and dark mode, and
each monitor can have its own.

<ConfigTable path="wallpaper" />

Beyond the path rules in [the guide](/guide/wallpaper), a relative image path is also tried in the
installed `wallpapers` data directory. PNG, JPEG, WebP, GIF and BMP render. `color` takes any CSS
color string, and one that does not parse becomes opaque black. A bad color, a missing image file
and an unreadable one are only logged, never caught by `glimpsectl config validate`; [the
guide](/guide/wallpaper#if-the-picture-does-not-change) covers what shows for the file cases.

<ConfigTable path="wallpaper.outputs" describe />

<ConfigTable path="wallpaper.backdrop" describe />

The backdrop is a blurred copy of the wallpaper on its own surface, created by default. It shows
only where your compositor places it; in `niri` this layer rule puts it behind the Overview:

```kdl
// ~/.config/niri/config.kdl
layer-rule {
    match namespace="^glimpse-backdrop$"
    place-within-backdrop true
}
```

A different image for an external monitor, in both modes:

```toml
[wallpaper]
image = "~/Pictures/walls/dunes.jpg"
image-dark = "~/Pictures/walls/dunes-night.jpg"

[[wallpaper.outputs]]
monitor = "DP-1"
image = "~/Pictures/walls/coast.jpg"
image-dark = "~/Pictures/walls/coast-night.jpg"
```
