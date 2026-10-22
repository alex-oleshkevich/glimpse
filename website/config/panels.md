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

## Default panel

| Zone | Applets |
| --- | --- |
| `left` | `workspace-name`, `pager`, `places`, `mpris` |
| `center` | `clock`, `weather`, `notifications`, `privacy` |
| `right` | `next-event`, `tray`, `removable`, `clipboard`, `printing`, `bluetooth`, `network`, `display`, `audio`, `idle`, `power`, `keyboard`, `session` |

See the [applet reference](./applets/) for what each one does.
