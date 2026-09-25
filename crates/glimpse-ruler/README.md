# glimpse-ruler

A command that freezes the screen under an overlay, measures chained line segments through a zoom
lens, prints the measurements as NDJSON on stdout, and exits.

```
glimpse-ruler [--lens-radius PIXELS] [--max-zoom TIMES] [--config PATH]
```

Each confirmed segment produces one JSON line in confirmation order. For a segment from `(0, 0)` to
`(3, 0)`, the line is:

```json
{"angle":0.0,"distance":3.0,"dx":3,"dy":0,"from_x":0,"from_y":0,"to_x":3,"to_y":0}
```

Exit code 0 means at least one segment was measured; 4 means nothing was measured or an output went
away; 3 means the configuration failed to load; 1 means another failure. Errors go to stderr.

## Configuration

`[ruler]` accepts `history-limit` (default 8, clamped to 1–64 recent measurements in the panel),
`lens-radius` (default 106, clamped to 40–400 logical pixels), and `max-zoom` (default 30, clamped
to 2–64 times magnification). The two lens flags override their corresponding config keys.

## Rules

**Capture before opening the overlay.** `glimpse_wayland::capture::outputs` captures each output once
without the cursor. The overlay paints those frames at logical size, so neither the ruler nor its
lens appears in a measurement.

**Keep segments on their own output.** The first click anchors a point; each later click confirms a
segment and becomes the next anchor. Moving to another output drops the pending anchor without
joining points across outputs. Esc ends the session, and a right click clears the chain.

**Use the captured buffer for coordinates.** The lens maps logical pointer positions to buffer
pixels, including fractional output scales. Shift constrains the next point to the horizontal or
vertical axis. The scroll wheel zooms the lens; Shift plus scroll changes its radius.
