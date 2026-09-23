# glimpse-picker

A command: freeze the screen, pick a pixel through a zoom lens, print it on stdout, exit.

```
glimpse-picker [--format hex|rgb|hsl|hsv|oklch|cmyk] [--json] [--lens-radius PX] [--max-zoom N]
```

Every flag falls back to `[color-picker]` in the configuration. `--json` prints one object —
`{"format":"hex","value":"#E0563F","red":224,"green":86,"blue":63}` — and plain output is the
value alone. It exits 0 with a color, 4 when the lens was cancelled, 3 on a configuration error and
1 otherwise, with the reason on stderr. It owns no clipboard, since a selection dies with the process
that set it: pipe it to `wl-copy`, or pick through the panel's `color-picker` applet, whose service
runs this command with `--json` and puts the result on the clipboard itself.

## Rules

**The screen is frozen, not live.** Every output is captured once through
`zwlr_screencopy_manager_v1` with `overlay_cursor = 0` before GTK starts, so the lens never shows
itself. Each output gets a `Layer::Overlay` surface anchored on all edges with
`KeyboardMode::Exclusive`, painting its frame at logical size under a hidden cursor.

**Every capture wait has one deadline.** The compositor is polled through `prepare_read`, never
`blocking_dispatch`, and the whole capture runs on a thread the process waits for with a timeout, so
a compositor that never answers ends the pick instead of hanging it.

**An output without a name refuses the pick,** and so does a monitor with no captured frame: monitors
are matched to frames by connector, which `wl_output` carries only from version 4, and a monitor
with no surface would pass clicks through to the desktop.

**A pick reads the captured buffer, never the drawn texture.** `pixel_at` maps a logical pointer
position to a buffer pixel, which at a fractional scale is not the same pixel as a rounded logical
one. The frame is turned upright at capture — the output transform and `y_invert` applied once —
so nothing downstream knows about either.

**The lens is a circle of `lens-radius`; the scroll wheel zooms it from ×1 to `max-zoom`**, opening
at ×8. A notch multiplies or divides by 1.25, at least one step, so ×1 to ×30 is about a dozen
notches. The pixel grid appears from ×6. Arrow keys nudge one buffer pixel, since Wayland cannot warp
the pointer; Enter or a left click picks, Esc or a right click cancels, and so does an output going
away. The crosshair is black and white on purpose: it sits on arbitrary content and must read on any
of it.

**Only 8-bit buffers are read.** `Xrgb8888`, `Argb8888`, `Xbgr8888` and `Abgr8888`; any other buffer
format refuses the pick with its name rather than guessing at a layout.
