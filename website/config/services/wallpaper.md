# Wallpaper

`[wallpaper]` sets the desktop background. An image can differ between light and dark mode, and
each monitor can have its own.

<ConfigTable path="wallpaper" />

<ConfigTable path="wallpaper.outputs" describe />

<ConfigTable path="wallpaper.backdrop" describe />

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
