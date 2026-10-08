# glimpse-lens

`glimpse-lens` magnifies the frozen screen, exits successfully when dismissed, and writes no result.
It uses `[color-picker]` lens settings by default; `--tool picker` picks a color and `--tool ruler`
measures chained line segments using their respective configuration tables. `--format` and `--json`
require `--tool picker`.

```
glimpse-lens [--tool picker|ruler] [--lens-radius PIXELS] [--max-zoom TIMES] [--config PATH]
glimpse-lens --tool picker [--format hex|rgb|hsl|hsv|oklch|cmyk] [--json]
glimpse-lens --tool ruler [--lens-radius PIXELS] [--max-zoom TIMES] [--config PATH]
```

## Color picker

Flags fall back to `[color-picker]` in the configuration. `--json` prints one object —
`{"format":"hex","value":"#E0563F","red":224,"green":86,"blue":63}` — and plain output is the
value alone. Exit code 0 means a color was picked; 4 means the lens was cancelled or an output went
away; 3 means the configuration failed to load; 1 means another failure. Errors go to stderr.

The command owns no clipboard, since a selection dies with the process that set it. Pipe its output
to `wl-copy`.

Enter or a left click picks; Esc or a right click cancels. Arrow keys nudge one buffer pixel, since
Wayland cannot warp the pointer. The crosshair is black and white so it reads on arbitrary content.

## Ruler

Each confirmed segment produces one JSON line in confirmation order:

```json
{"angle":0.0,"distance":3.0,"dx":3,"dy":0,"from_x":0,"from_y":0,"to_x":3,"to_y":0}
```

Exit code 0 means at least one segment was measured; 4 means nothing was measured or an output went
away; 3 means the configuration failed to load; 1 means another failure. Errors go to stderr.

`[ruler]` accepts `history-limit` (default 8, clamped to 1–64 recent measurements in the panel),
`lens-radius` (default 106, clamped to 40–400 logical pixels), and `max-zoom` (default 30, clamped
to 2–64 times magnification). The two lens flags override their corresponding config keys.

Segments stay on their own output. The first click anchors a point; each later click confirms a
segment and becomes the next anchor. Moving to another output drops the pending anchor without
joining points across outputs. Esc ends the session, and a right click clears the chain. Shift
constrains the next point to the horizontal or vertical axis. Shift plus scroll changes the radius.

## Capture and rendering

`glimpse_wayland::capture::outputs` captures every output once without the cursor before GTK starts,
so the overlay never captures itself. Each output gets a `Layer::Overlay` surface anchored on all
edges with `KeyboardMode::Exclusive`, painting its frame at logical size under a hidden cursor.
The capture runs on a thread with a deadline so an unresponsive compositor cannot hang the command.

Monitors match frames by connector, which `wl_output` carries only from version 4. An unnamed output,
a monitor without a frame, or a buffer the capture cannot read refuses the operation.

Picks and measurements read captured buffer coordinates, including fractional scales. Frames arrive
upright, so the tools do not handle output transforms. Capture and the lens widget are shared through
`glimpse-wayland` and `glimpse-widgets`.

The lens opens at ×8. The scroll wheel zooms from ×1 to `max-zoom`, multiplying or dividing by 1.25
per notch with at least one step. The pixel grid appears from ×6.
