# glimpse-wallpaper

The background layer surface: sources, decoding, transitions and the overview backdrop.

A separate binary from the panel so that restarting the panel does not black the screen.

## What it does

- One background surface per output, redrawn across hotplug and scale changes
- Single image or a directory cycled on an interval
- Follows `appearance.scheme` to swap light and dark sources

## Rules

An output announced with zero geometry is deferred until its geometry arrives, never skipped and
never given a surface. Decode targets are `geometry × scale_factor`, not the logical size.

The decoded-image cache is keyed on everything that changes the output pixels — format version,
path, size and mtime, fit, blur radius, target size. Writes are atomic, the cache is swept, and a
watcher report beats the mtime signature.

A crossfade loads into the hidden slot and clears the replaced one on a timer; every load carries a
request id so a stale decode cannot overwrite a newer image.

With the daemon down, it keeps drawing the last image and the last theme mode.

Configuration is the `[wallpaper]` and `[backdrop]` tables of the shared `config.toml`. Tables owned by other
binaries are ignored, not validated. Schema in
[`specs/010_configuration.md`](../../specs/010_configuration.md).

Spec: [`specs/005_wallpaper.md`](../../specs/005_wallpaper.md)
