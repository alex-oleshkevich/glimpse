# Arrange your panels

The bar is one or more `[[panels]]` tables in `~/.config/glimpse/panel.toml`. Save the file and the
change applies. The full setting list is in the [panels reference](/config/panels).

::: tip
The moment you write your first `[[panels]]` table, the default panel is gone and glimpse shows only
the panels you wrote.
:::

## Reorder applets

Each panel has three zones, `left`, `center` and `right`, and each lists applet names in the order
they appear on screen. To move an applet, move its name. A zone you write replaces the whole default
list, so a name you leave out disappears from the bar. This example keeps every default applet and
moves the clock to the end of the center zone:

```toml
[[panels]]
left = ["workspace-name", "pager", "places", "mpris"]
center = ["weather", "notifications", "privacy", "clock"]
right = [
    "next-event", "tray", "removable", "clipboard", "printing", "bluetooth",
    "network", "display", "audio", "idle", "power", "keyboard", "session",
]
```

## Move the bar to another edge

`position` takes `top` (the default), `bottom`, `left` or `right`:

```toml
[[panels]]
position = "bottom"
```

## Change the bar's size

`size` is the least thickness in pixels: the height of a top or bottom bar, the width of a side
bar. The default is 36. A bar is never thinner than its tallest applet, about 30 pixels, and a side
bar is as wide as its widest applet, so a clock label makes it about 120 pixels wide.

```toml
[[panels]]
size = 42
```

A larger `size` applies when you save. A smaller one takes effect the next time the panel starts.

## Float the bar

A `[panels.margin]` table pulls the bar away from the screen edges, in pixels. It belongs to the
`[[panels]]` table above it, and windows keep clear of the bar and its margin:

```toml
[[panels]]
position = "top"

[panels.margin]
top = 8
left = 12
right = 12
```

## Put a second bar on another monitor

Add one `[[panels]]` table per bar and name the monitor by its connector. This keeps the full bar on
the laptop screen and adds a smaller one on an external monitor:

```toml
[[panels]]
monitor = "eDP-1"

[[panels]]
monitor = "DP-1"
size = 30
left = ["pager"]
center = ["clock"]
right = ["notifications"]
```

A panel without `monitor` appears on every monitor, and a zone you leave out keeps its default
applets. A panel whose `monitor` matches no connected monitor does not appear, and glimpse does not
warn you, so check the spelling. `glimpsectl applet list` lists every applet with the connector its panel is on.

## Remove an applet

Delete its name from the zone list. To empty a zone completely, set it to `[]`:

```toml
[[panels]]
center = []
right = ["tray", "network", "audio"]
```

A name glimpse cannot resolve (a typo, an applet that is not installed) is left out, and the panel
posts a notification telling you which one.
