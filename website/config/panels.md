# Panels

Each `[[panels]]` table describes one panel. Without any, glimpse shows the default panel along the
top of every monitor. Writing `[[panels]]` tables replaces the default panel with yours, one panel
per table. A setting you leave out of a table keeps its default, including the zones: a panel that
sets only `left` still gets the default `center` and `right` applets. Set a zone to `[]` to empty it,
and write `panels = []` to have no panel at all.

Panels live in `panel.toml`, with `[applets]`. They are the one thing that does not merge across
files: the last file with any `[[panels]]` table defines all of your panels, so a drop-in in
`panel.d/` cannot add a panel to the ones in `panel.toml`.

A panel has three zones, `left`, `center` and `right`. Each lists applet names in the order they
appear. On a side panel `left` is the top end and `right` the bottom end. A name is either a built-in applet such as `clock`, a name you defined under
`[applets.<name>]`, or the id of an installed exec applet. The panel leaves out a name it can't resolve
and tells you which one in a notification. A name with a dot, such as `me.aresa.GlimpseTodo`, is read
as an exec applet id; if no such applet is installed, its slot stays empty without a notification.

```toml
[[panels]]
position = "top"
size = 36
left = ["workspace-name", "pager", "places", "mpris"]
center = ["clock", "weather", "notifications", "privacy"]
right = ["tray", "bluetooth", "network", "audio", "power", "session"]
```

## Settings

<ConfigTable path="panels" />

<ConfigTable path="panels.margin" />

## Several panels

Add a `[[panels]]` table for each panel. A `[panels.margin]` table belongs to the `[[panels]]` above
it. This puts a full panel on the laptop screen and a smaller one on an external monitor:

```toml
[[panels]]
monitor = "eDP-1"
left = ["workspace-name", "pager"]
center = ["clock"]
right = ["tray", "network", "audio", "power", "session"]

[[panels]]
monitor = "DP-1"
size = 30
left = ["pager"]
center = ["clock"]
right = ["notifications"]
```

`monitor` takes a connector name such as `eDP-1` or `DP-1`, matched exactly. A panel whose `monitor`
matches no connected monitor does not appear, and glimpse does not warn you. `glimpsectl applet list`
lists every applet on screen with the connector its panel is on; while no panel shows, your
compositor's own output list (for example `niri msg outputs`) gives the names.

## Hiding the panel

A panel slides out to its own edge and back. `visibility = "hide"` starts it hidden;
`glimpsectl panel` changes it while it runs:

| Command | What it does |
| --- | --- |
| `show` | Slides the panel in and reserves its space, so windows sit beside it |
| `hide` | Slides the panel out and gives its space back to windows |
| `toggle` | `show` on a hidden panel, `hide` on a shown one |
| `reveal` | Slides a hidden panel in over windows, which do not move, until `conceal` |
| `reveal --for 2000` | The same, for two seconds |
| `conceal` | Ends a `reveal` |

Each command takes `--output` and `--edge` to pick a panel; without them it applies to every panel.
A change lasts until the panel restarts or you edit `[[panels]]`.

A revealed panel stays while the pointer is on it or one of its popovers is open, and slides out
`hide-delay-ms` after both end. With `edge-reveal = true`, resting the pointer on the screen edge of
a hidden panel reveals it:

```toml
[[panels]]
visibility = "hide"
edge-reveal = true
hide-delay-ms = 1000
```

Bind the commands to keys in your compositor. In niri:

```kdl
binds {
    Mod+B { spawn "glimpsectl" "panel" "reveal" "--for" "2000"; }
    Mod+Shift+B { spawn "glimpsectl" "panel" "toggle"; }
}
```

glimpse does not watch your compositor for this. To show the panel while the niri overview is
open, run a script from `spawn-at-startup`:

```sh
niri msg --json event-stream \
  | jq --unbuffered -r '.OverviewOpenedOrClosed.is_open // empty' \
  | while read -r open; do
      if [ "$open" = true ]; then glimpsectl panel reveal; else glimpsectl panel conceal; fi
    done
```

A compositor rule that draws a shadow or blur behind the panel's surface, such as Hyprland's
`layerrule = blur`, still draws it while the panel is hidden, since the surface stays in place.

## Default panel

| Zone | Applets |
| --- | --- |
| `left` | `workspace-name`, `pager`, `places`, `mpris` |
| `center` | `clock`, `weather`, `notifications`, `privacy` |
| `right` | `next-event`, `tray`, `removable`, `clipboard`, `printing`, `bluetooth`, `network`, `display`, `audio`, `idle`, `power`, `keyboard`, `session` |

See the [applet reference](./applets/) for what each one does.
