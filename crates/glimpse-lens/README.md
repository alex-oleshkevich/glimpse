# glimpse-lens

`glimpse-lens` magnifies the frozen screen, exits successfully when dismissed, and writes no result.
`--tool picker` picks a color and `--tool ruler` measures chained line segments. Options come only
from flags: `--radius` defaults to 180 logical pixels, clamped to 10–200, and `--zoom` sets opening
magnification, defaulting to ×3 and clamped to 1–200. `--format` and `--json` require `--tool picker`.

```
glimpse-lens [--tool picker|ruler] [--radius PIXELS] [--zoom TIMES]
glimpse-lens --tool picker [--format hex|rgb|hsl|hsv|oklch|cmyk] [--json] [--copy]
glimpse-lens --tool ruler [--radius PIXELS] [--zoom TIMES] [--copy]
```

## Color picker

The picker defaults to hex output. `--json` prints one object —
`{"blue":63,"format":"hex","green":86,"red":224,"value":"#E0563F"}` — and plain output is the
value alone. Exit code 0 means a color was picked; 4 means the lens was cancelled or an output went
away; 1 means another failure. Errors go to stderr.

`--copy` requires `--tool`. After writing stdout, it sends the printed line (picker) or the last
segment's distance in whole pixels (ruler) to `me.aresa.Glimpse.Clipboard1.CopyText`, and exits 1 if
the panel does not answer within 3 seconds.

Enter or a left click picks; Esc or a right click cancels. Arrow keys nudge one buffer pixel, since
Wayland cannot warp the pointer. The crosshair is black and white so it reads on arbitrary content.

## Ruler

Each confirmed segment produces one JSON line in confirmation order:

```json
{"angle":0.0,"distance":3.0,"dx":3,"dy":0,"from_x":0,"from_y":0,"to_x":3,"to_y":0}
```

Exit code 0 means at least one segment was measured; 4 means nothing was measured or an output went
away; 1 means another failure. Errors go to stderr.

Segments stay on their own output. The first click anchors a point; each later click confirms a
segment and becomes the next anchor. Moving to another output drops the pending anchor without
joining points across outputs. Esc ends the session, and a right click clears the chain. Shift
constrains the next point to the horizontal or vertical axis.

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

The scroll wheel zooms from ×1 to ×200, multiplying or dividing by 1.25 per notch with at least one
step. Shift plus scroll resizes the lens in both tools, multiplying or dividing the radius by 1.15 per
notch with at least 4 pixels, within 10–200. The pixel grid appears from ×6.
