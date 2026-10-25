# glimpse-wallpaper

The background layer surface: sources, decoding, transitions and the overview backdrop.

A separate binary from the panel so that restarting the panel does not black the screen.

## What it does

- Per output, a background and (unless disabled) a backdrop surface, redrawn on hotplug and rescale
- A single image, with an optional dark variant
- Follows `appearance.color-scheme` to swap between them
- Serves the wallpaper portal: an accepted image is copied under `state_dir()/wallpaper/`,
  written as `[wallpaper] image` and `image-dark`, and announced by a notification that can undo it

## Rules

Every output gets its surfaces when it is announced; one with zero geometry decodes nothing until
its geometry arrives. Decode targets are geometry × the monitor's fractional scale, falling back
to the integer scale factor only when the fractional scale is unavailable — not the logical size.

The cache lives on each surface, in memory: the currently-rendered texture, at most one in-flight
decode, and the outgoing texture held during a crossfade. The key is (image path, decode target,
fit, blur radius, mtime); dark mode is already resolved into which image path is used, so it plays
no part in the key itself.

The backdrop decodes at the output's physical size divided by `downscale-factor`, and is blurred
once, on the GPU, when its texture is applied: a gaussian `GskBlurNode` rendered offscreen through
the surface's own renderer. The radius is configured in output pixels and scaled to the texture.
Never blur by shrinking and re-stretching: that is a thumbnail, not a blur.

A crossfade fades the outgoing texture out and clears it when the fade ends; a decode is applied
only while its key is still the one the surface wants, so it cannot overwrite a newer image.

The image and the theme mode come from the configuration it reads itself, so no other process's
absence leaves a black screen. The overview backdrop follows the compositor provider and stays off
while no provider serves it.

Configuration is the `[wallpaper]` table of the shared `config.toml`, including its nested
`[wallpaper.backdrop]`. Tables owned by other binaries are ignored, not validated. Schema in
`glimpse-config/src/schema/wallpaper.rs`. A `[[wallpaper.outputs]]` entry overrides the global
values field by field, and the first entry naming a connector wins.
