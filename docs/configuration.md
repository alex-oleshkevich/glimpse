# Configuration

The main config file is `~/.config/glimpse/config.toml`. It controls the panel, applets, wallpaper, lock screen, location, night light, and idle behavior.

## Start With A Panel

This is a compact panel:

```toml
[[panels]]
monitor = "eDP-1"
position = "top"
size = 36
left = ["pager", "..."]
center = ["clock"]
right = ["network", "battery", "weather", "session"]
```

The special name `"..."` means "keep the default applets for this section here." It lets you add your own items without copying the whole default layout.

## Panel Options

| Option | What it does |
|---|---|
| `monitor` | Output name. Leave it out if one panel should appear on the default output. |
| `position` | `top`, `bottom`, `left`, or `right`. |
| `size` | Panel thickness in pixels. |
| `theme_mode` | Per-panel mode: `auto`, `dark`, or `light`. |
| `left` | Applets on the left side. |
| `center` | Applets in the center. |
| `right` | Applets on the right side. |

## Built-In Applets

| Applet | Use it for |
|---|---|
| `audio` | Volume status and controls. |
| `battery` | Battery percentage and charging status. |
| `bluetooth` | Bluetooth status and devices. |
| `display` | Display brightness. |
| `clipboard` | Clipboard history. |
| `clock` | Time and calendar. |
| `keyboard` | Current keyboard layout. |
| `mpris` | Media player status. |
| `network` | Wi-Fi and wired network status. |
| `next_event` | Next upcoming calendar event. |
| `notifications` | Notification center. |
| `pager` | Workspaces and windows. |
| `privacy` | Camera, microphone, and screen sharing indicators. |
| `removable` | USB and removable drives. |
| `session` | Lock, logout, suspend, restart, and shutdown actions. |
| `tray` | Status notifier icons. |
| `weather` | Current weather. |
| `command` | A button or menu that runs commands. |
| `exec` | A live custom applet powered by your own script. |

## Configure An Applet

### Configure an applet

Applet settings for a built-in applet live under `[applets.name]`.

```toml
[applets.weather]
city_name = "Warsaw, PL"
```

Place the applet name in a panel section:

```toml
right = ["weather", "network", "battery"]
```

### Add another instance

Built-in applet settings live under `[applets.<name>]`. Custom `exec` and
`command` applets are loaded from applet package files in
`$XDG_CONFIG_HOME/glimpse/applets`.

```toml
# ~/.config/glimpse/applets/terminal.toml
id = "terminal"
type = "command"

[command]
icon = "utilities-terminal-symbolic"
tooltip = "Open terminal"
command = ["ghostty"]
```

Then place the package id in a panel section:

```toml
right = ["weather", "terminal", "network", "battery"]
```

## Calendar Sources

Calendar sources live in the shared `[calendar]` section. They are used by the clock popover and the `next_event` applet. Every configured source is active; remove a source block to disable that calendar.

```toml
[calendar]
poll_interval = 600

[[calendar.sources]]
id = "personal"
type = "ical"
name = "Personal"
uri = "https://calendar.google.com/calendar/ical/example/basic.ics"
color = "#4285f4"

[[calendar.sources]]
id = "private-url-file"
type = "ical"
name = "Private URL File"
uri = "file:///home/alex/.config/glimpse/calendars/private.url"
color = "#e01b24"

[[calendar.sources]]
id = "local"
type = "directory"
name = "Local"
uri = "file:///home/alex/.config/glimpse/calendars"
color = "#f6c343"
```

Read [Calendar Sources](./calendar.md) for source types, polling, URL sidecar files, local test events, recurrence support, display rules, and debugging.

## Location

Location is used by weather and automatic night light.

For automatic location:

```toml
[location]
provider = "geoclue"
```

For a fixed place:

```toml
[location]
provider = "static"
latitude = 52.2297
longitude = 21.0122
```

Static location is useful on desktops, privacy-focused setups, or systems without a location provider.

## Keep Config Tidy

Use one section per topic:

```toml
theme = "adwaita"
theme_mode = "auto"

[[panels]]
position = "top"
size = 36
left = ["pager"]
center = ["clock"]
right = ["network", "battery", "session"]

[location]
provider = "static"
latitude = 52.2297
longitude = 21.0122

[applets.clock]
label_format = "%H:%M"

[calendar]
poll_interval = 600

[[calendar.sources]]
id = "personal"
type = "ical"
name = "Personal"
uri = "file:///home/alex/.config/glimpse/calendars/personal.ics"
color = "#4285f4"

[wallpaper]
path = "/home/alex/Pictures/wallpapers/coast.jpg"
fit = "cover"

[lock]
css_path = "themes/lock.css"
```

`theme` and `theme_mode` are independent: `theme` selects the CSS file, and `theme_mode` selects automatic, dark, or light mode classes.

Read [Applets](./applets/) for per-applet options, [Calendar Sources](./calendar.md) for event feeds, [Wallpaper](./wallpaper.md) for background settings, and [Lock](./lock.md) for lock screen settings.
