# glimpse-wallpaper

The background layer surface and its live effects.

A separate binary from the panel for two reasons: restarting the panel should not black the screen,
and a GL render loop should not share a process with panel layout work.

## What it does

- One background surface per output, redrawn across hotplug and scale changes
- Single image or a directory cycled on an interval
- Follows `theme.mode` to swap light and dark sources
- Live effects drawn over the base image

## Rules

An effect must be frame-budget aware. Repeatedly missing frames disables the effect and logs it
rather than degrading the whole session.

With the daemon down, it keeps drawing the last image and the last theme mode.

Spec: [`specs/005_glimpse_wallpaper.md`](../../specs/005_glimpse_wallpaper.md)
