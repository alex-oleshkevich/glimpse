# glimpse-wallpaper

The background layer surface: sources, decoding, transitions and the overview backdrop.

A separate binary from the panel so that restarting the panel does not black the screen.

## What it does

- One background surface per output, redrawn across hotplug and scale changes
- A single image, with an optional dark variant
- Follows `appearance.color-scheme` to swap between them

## Rules

An output announced with zero geometry is deferred until its geometry arrives, never skipped and
never given a surface. Decode targets are geometry × the monitor's fractional scale, falling back
to the integer scale factor only when the fractional scale is unavailable — not the logical size.

The cache lives on each surface, in memory: the currently-rendered texture, at most one in-flight
decode, and the outgoing texture held during a crossfade. The key is (image path, decode target,
fit, blur radius, mtime); dark mode is already resolved into which image path is used, so it plays
no part in the key itself.

The backdrop decodes at the output's physical size divided by `downscale-factor`, and is blurred
once, on the GPU, when its texture is applied: a gaussian `GskBlurNode` rendered offscreen through
the surface's own renderer. The radius is configured in output pixels and scaled to the texture.
Never blur by shrinking and re-stretching: that is a thumbnail, not a blur.

A crossfade loads into the hidden slot and clears the replaced one on a timer; every load carries a
request id so a stale decode cannot overwrite a newer image.

It depends on no other glimpse process: the image and the theme mode come from the configuration it
reads itself, so there is nothing whose absence leaves a black screen.

Configuration is the `[wallpaper]` table of the shared `config.toml`, including its nested
`[wallpaper.backdrop]`. Tables owned by other binaries are ignored, not validated. Schema in
`glimpse-config/src/schema/wallpaper.rs`.
