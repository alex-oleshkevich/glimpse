# Panels

Each `[[panels]]` table describes one panel. Without any, glimpse shows the default panel along the
top of every monitor. Writing `[[panels]]` tables replaces the default panel with yours, one panel
per table. A setting you leave out of a table keeps its default, including the zones: a panel that
sets only `left` still gets the default `center` and `right` applets. Set a zone to `[]` to empty it.

A panel has three zones, `left`, `center` and `right`. Each lists applet names in the order they
appear. A name is either a built-in applet such as `clock`, a name you defined under
`[applets.<name>]`, or the id of an installed exec applet. The panel leaves out a name it can't resolve
and tells you which one in a notification.

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

`monitor` takes a connector name such as `eDP-1` or `DP-1`. `glimpsectl panel applets` lists every
applet on screen with the connector its panel is on.

## Default panel

| Zone | Applets |
| --- | --- |
| `left` | `workspace-name`, `pager`, `places`, `mpris` |
| `center` | `clock`, `weather`, `notifications`, `privacy` |
| `right` | `next-event`, `tray`, `removable`, `clipboard`, `color-picker`, `printing`, `bluetooth`, `network`, `display`, `audio`, `idle`, `power`, `keyboard`, `session` |

See the [applet reference](./applets/) for what each one does.
