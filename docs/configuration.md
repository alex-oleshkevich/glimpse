# Configuration

Glimpse is configured with one TOML file. Start with the panel layout, then add applet settings as you need them.

## Config file

Most users create this file:

```txt
~/.config/glimpse/config.toml
```

Use it for panel layout, applets, wallpaper, lock screen, idle rules, location, and night light. Put shell and lock CSS in `~/.config/glimpse/themes/`.

Glimpse looks for config in this order:

| Priority | Path |
|---|---|
| **1** | `GLIMPSE_CONFIG` environment variable |
| **2** | `./config.toml` in the current directory |
| **3** | `$XDG_CONFIG_HOME/glimpse/config.toml` |
| **4** | `$HOME/.config/glimpse/config.toml` when `XDG_CONFIG_HOME` is unset |

### Include other config files

Use top-level `include` to split shared settings into smaller TOML files:

```toml
include = ["common.toml", "laptop.toml"]

theme = "rosepine"

[[panels]]
left = ["workspace", "..."]
right = ["tray", "session"]
```

Included files load first, in list order, and the file that declares `include` loads last. That means the main `config.toml` remains the final override. Relative include paths resolve from the file that declares them, so an include inside `profiles/work.toml` is relative to `profiles/`. Glimpse watches included files for hot reload, so changing an included file reloads the merged config.

Tables merge recursively. Arrays and scalar values replace earlier values instead of appending:

```toml
# common.toml
[applets.clock]
label_format = "%H:%M"
tooltip_format = "%A"

[[panels]]
right = ["clock"]
```

```toml
# config.toml
include = ["common.toml"]

[applets.clock]
label_format = "%a %H:%M"

[[panels]]
right = ["tray", "session"]
```

The final clock config keeps `tooltip_format = "%A"` from `common.toml`, overrides `label_format`, and replaces the whole `panels` array with the panel from `config.toml`.

## Start with a panel

This is a compact top panel:

```toml
theme = "adwaita"
theme_mode = "auto"

[[panels]]
position = "top"
size = 36
left = ["pager", "..."]
center = ["clock"]
right = ["network", "battery", "session"]
```

The special name `"..."` keeps the default applets for that section in place. Use it when you want to add or remove a few items without copying the whole default layout.

## Theme and mode

Use `theme` to choose the theme pack and `theme_mode` to choose how Glimpse resolves light and dark styling:

```toml
theme = "rosepine"
theme_mode = "auto"
```

| Option | Default | Values | What it does |
|---|---|---|---|
| `theme` | `"adwaita"` | Theme pack name | Selects a pack by directory name. If the pack cannot be found, Glimpse still loads the built-in base style. |
| `theme_mode` | `"auto"` | `auto`, `dark`, `light` | Selects the effective light/dark mode used by CSS, theme images, wallpaper, backdrop, and lock background resolution. |

`theme_mode = "dark"` and `theme_mode = "light"` force that mode and ignore solar data.

For the panel, `theme_mode = "auto"` lets Glimpse choose the effective mode:

| State | Effective mode |
|---|---|
| Solar data says it is daytime | `light` |
| Solar data says it is nighttime | `dark` |
| Solar data is temporarily unavailable | Keeps the previous effective mode |
| Solar data has never been available in this session | `light` |

The panel also publishes the effective mode to the desktop color-scheme setting, so GTK applications that follow the system preference can switch with it.

The lock screen uses the same `theme_mode` setting. With `dark` or `light`, it uses that mode directly. With `auto`, it follows the desktop color-scheme setting, which normally reflects the panel's resolved solar mode.

Each panel can override only its own mode:

```toml
[[panels]]
position = "top"
theme_mode = "dark"
left = ["pager"]
center = ["clock"]
right = ["network", "battery", "session"]
```

Panel mode does not select a different theme pack. It only changes the light/dark styling for that panel.

## Panel options

| Option | What it does |
|---|---|
| `monitor` | Output name, such as `eDP-1` or `DP-1`. Leave it out for the default output. |
| `position` | Panel edge: `top`, `bottom`, `left`, or `right`. |
| `size` | Panel thickness in pixels. |
| `theme_mode` | Optional per-panel mode: `auto`, `dark`, or `light`. This changes only that panel's light/dark styling. |
| `left` | Applets on the left side. |
| `center` | Applets in the center. |
| `right` | Applets on the right side. |

To pin a panel to a specific monitor:

```toml
[[panels]]
monitor = "eDP-1"
position = "top"
size = 36
left = ["pager"]
center = ["clock"]
right = ["network", "battery", "session"]
```

## Built-in applets

Put these names in `left`, `center`, or `right`:

| Applet | Use it for |
|---|---|
| `audio` | Volume status and controls. |
| `battery` | Battery percentage and charging status. |
| `bluetooth` | Bluetooth status and devices. |
| `clipboard` | Clipboard history. |
| `clock` | Time and calendar. |
| `display` | Display brightness. |
| `idle` | Idle inhibitor status. |
| `keyboard` | Current keyboard layout. |
| `mpris` | Media player status. |
| `network` | Wi-Fi and wired network status. |
| `next_event` | Next upcoming calendar event. |
| `notifications` | Notification center. |
| `pager` | Workspaces and windows. |
| `printing` | Printer and print job status. |
| `privacy` | Camera, microphone, and screen sharing indicators. |
| `removable` | USB and removable drives. |
| `session` | Lock, logout, suspend, restart, and shutdown actions. |
| `tray` | Status notifier icons. |
| `weather` | Current weather. |
| `command` | A custom button or menu that runs commands. |
| `exec` | A live custom applet powered by your own program. |

`__dynamic__` is reserved for Glimpse's dynamic island area. Keep it in your panel if you want Glimpse to place dynamic status content automatically.

`__dev__` shows applets started with `glimpse-shell applets dev`. The default panel keeps it at the end of the left section.

## Configure an applet

Settings for a built-in applet live under `[applets.name]`.

```toml
[applets.clock]
label_format = "%H:%M"

[applets.weather]
city_name = "Warsaw, PL"
label_format = "{temp}"
```

Then place the applet names in a panel section:

```toml
right = ["weather", "network", "battery", "session"]
```

Read [Applets](./applets/) for the full list of options and format placeholders.

## Add a launcher button

Custom `command` and `exec` applets live in package files under:

```txt
~/.config/glimpse/applets/
```

This creates a terminal launcher:

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
right = ["weather", "terminal", "network", "battery", "session"]
```

Use `exec` when you want a live applet with changing text, icons, popovers, or click handling. Read [Custom Applets](./custom-applets/) for the applet package format and SDKs.

## Calendar sources

Calendar sources live in the shared `[calendar]` section. The clock popover and `next_event` applet both use these sources.

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
id = "local"
type = "directory"
name = "Local"
uri = "file:///home/alex/.config/glimpse/calendars"
color = "#f6c343"
```

Use `type = "ical"` for provider `.ics` URLs or local `.ics` files, and `type = "directory"` for a folder of local calendar files. Read [Calendar Sources](./calendar.md) for Google Calendar and Outlook ICS links, optional sidecar URL files, recurrence support, display rules, and debugging.

## Location

Shared location is used by weather when `city_name` is empty, and by automatic night light.

```toml
[location]
provider = "geo_clue"
```

For a fixed weather place, set the weather applet directly:

```toml
[applets.weather]
city_name = "Warsaw, PL"
```

Static shared coordinates are not a supported `[location]` provider in the current config format.

## Services

The same config file also controls the background services for night light and idle rules. Enable the services from [Installation](./installation.md), then tune their behavior here.

### Night light

Night light warms the display after dark. The default config follows sunrise and sunset from the shared location provider:

```toml
[location]
provider = "geo_clue"

[night_light]
schedule = "automatic"
temperature = 4200
transition_minutes = 15
```

| Option | Default | Values | What it does |
|---|---|---|---|
| `schedule` | `"automatic"` | `automatic`, `schedule`, `off` | Chooses automatic solar timing, fixed clock times, or no color change. |
| `temperature` | `4200` | Kelvin value | Sets how warm the screen becomes. Lower values look warmer. |
| `start_time` | unset | `HH:MM` | Start time for `schedule = "schedule"`. |
| `end_time` | unset | `HH:MM` | End time for `schedule = "schedule"`. |
| `transition_minutes` | `15` | Minutes | Fades between normal color and night-light color. |

Use fixed hours when location is not available or you want the same schedule every day:

```toml
[night_light]
schedule = "schedule"
start_time = "20:30"
end_time = "07:00"
temperature = 4200
transition_minutes = 15
```

Temperature is measured in Kelvin:

| Value | Feel |
|---|---|
| `6500` | Normal daylight color. |
| `5000` | Slightly warm. |
| `4200` | Comfortable evening warmth. |
| `3500` | Very warm late-night color. |

If nothing changes, check that `schedule` is not `off`, that the background service is running, and that your compositor supports the Wayland gamma-control protocol.

### Idle

Idle rules decide what happens after no keyboard or mouse input. The default profile is a laptop-friendly ladder:

| Step | AC | Battery |
|---|---|---|
| Turn monitors off | 10 minutes | 5 minutes |
| Lock session | 15 minutes | 15 minutes |
| Suspend | 60 minutes | 30 minutes |

Monitors come back on automatically when input resumes. The packaged monitor helper supports niri and Hyprland.

```toml
[idle]
enabled = true
respect_inhibitors = true

[idle.profiles.ac]
listeners = [
  { timeout = 600, on_idle = "/usr/share/glimpse/scripts/monitors off", on_resume = "/usr/share/glimpse/scripts/monitors on" },
  { timeout = 900, on_idle = "loginctl lock-session" },
  { timeout = 3600, on_idle = "systemctl suspend" },
]

[idle.profiles.battery]
listeners = [
  { timeout = 300, on_idle = "/usr/share/glimpse/scripts/monitors off", on_resume = "/usr/share/glimpse/scripts/monitors on" },
  { timeout = 900, on_idle = "loginctl lock-session" },
  { timeout = 1800, on_idle = "systemctl suspend" },
]
```

| Option | Default | What it does |
|---|---|---|
| `enabled` | `true` | Turns idle rules on or off. |
| `respect_inhibitors` | `true` | Honors apps that ask the desktop to stay awake. |
| `profiles.ac.listeners` | Default AC ladder | Rules used while plugged in. |
| `profiles.battery.listeners` | Default battery ladder | Rules used on battery. |

Each listener has these fields:

| Field | Required | What it does |
|---|---|---|
| `timeout` | Yes | Seconds of no keyboard or mouse input before the rule runs. |
| `on_idle` | Yes | Shell command to run after the timeout. |
| `on_resume` | No | Shell command to run when activity returns, but only after that listener has fired. |
| `respect_inhibitors` | No | Overrides the global inhibitor setting for this listener. |

Inhibitors are requests from apps such as video players, games, screen sharing tools, and presentation software. With `respect_inhibitors = true`, those apps can keep idle actions from firing while they need the screen awake.

For one rule that should always run, override it on that listener:

```toml
[idle.profiles.ac]
listeners = [
  { timeout = 900, on_idle = "loginctl lock-session", respect_inhibitors = false },
]
```

## Wallpaper and lock

Wallpaper and lock screen settings can stay in the same file:

```toml
[wallpaper]
path = "/home/alex/Pictures/wallpapers/coast.jpg"
fit = "cover"

[lock]
css_path = "themes/lock.css"
```

Read [Wallpaper](./wallpaper.md) for background settings and [Lock](./lock.md) for lock screen options.

## Default config reference

These snippets show defaults by file. You usually do not need to copy all of this. Copy only the section you want to change.

### `~/.config/glimpse/config.toml`

This is the main file. It controls the panel layout, built-in applet overrides, night light, idle behavior, keyboard layout memory, wallpaper, lock screen, monitors, and calendar sources.

```toml
theme = "adwaita"
theme_mode = "auto"

[location]
provider = "geo_clue"

[[panels]]
# monitor = "eDP-1"
size = 36
position = "top"
margin = { left = 0, right = 0, top = 0, bottom = 0 }
theme_mode = "dark"
left = ["pager", "mpris", "__dev__"]
center = ["clock", "weather", "notifications", "privacy"]
right = [
  "__dynamic__",
  "next_event",
  "tray",
  "removable",
  "clipboard",
  "keyboard",
  "printing",
  "bluetooth",
  "network",
  "display",
  "audio",
  "idle",
  "battery",
  "session",
]

[applets]

[night_light]
temperature = 4200
schedule = "automatic"
# start_time = "20:00"
# end_time = "06:00"
transition_minutes = 15

[idle]
enabled = true
respect_inhibitors = true

[idle.profiles.ac]
listeners = [
  { timeout = 600, on_idle = "/usr/share/glimpse/scripts/monitors off", on_resume = "/usr/share/glimpse/scripts/monitors on" },
  { timeout = 900, on_idle = "loginctl lock-session", on_resume = "" },
  { timeout = 3600, on_idle = "systemctl suspend", on_resume = "" },
]

[idle.profiles.battery]
listeners = [
  { timeout = 300, on_idle = "/usr/share/glimpse/scripts/monitors off", on_resume = "/usr/share/glimpse/scripts/monitors on" },
  { timeout = 900, on_idle = "loginctl lock-session", on_resume = "" },
  { timeout = 1800, on_idle = "systemctl suspend", on_resume = "" },
]

[keyboard]
remember = "window"

[wallpaper]
# path = "/home/alex/Pictures/wallpapers/coast.jpg"
color = "#101010"
fit = "cover"
transition_ms = 800

[backdrop]
enabled = true
# path = "/home/alex/Pictures/wallpapers/coast.jpg"
blur_radius = 24

[lock]
css_path = "themes/lock.css"

[lock.background]
# color = "#101010"
# path = "/home/alex/Pictures/wallpapers/lock.jpg"
# fit = "cover"
blur_radius = 0
dim = 0.35

[lock.clock]
enabled = true
time_format = "%H:%M"
date_format = "%A, %B %-d"

[lock.controls]
buttons = ["wifi", "input", "weather", "battery", "power"]

[monitors]
# builtin_connector = "eDP-1"

[calendar]
poll_interval = 600
sources = []
```

### Built-in applet sections in `~/.config/glimpse/config.toml`

Built-in applet sections are also written in the main config file. They are implicit; add a section only when you want to override it.

```toml
[applets.audio]
show_icon = true
show_muted_indicator = true
label_format = ""
tooltip_format = "{device} - {volume}%"
scroll_step = 10
max_volume = 100
show_streams = true

[applets.battery]
show_icon = true
label_on_battery = ""
label_on_ac = ""
tooltip_on_battery = "{percentage}% {state}, {time_left}"
tooltip_on_ac = "{percentage}% {state}"

[applets.bluetooth]
label_format = ""
tooltip_format = "{devices} connected devices"

[applets.clipboard]
label_format = ""
tooltip_format = "{count} clipboard items"
show_when_empty = false

[applets.clock]
label_format = "%a %-d %b, %H:%M"
tooltip_format = "%A, %-d %B %Y"
# [[applets.clock.timezones]]
# name = "New York"
# timezone = "America/New_York"
# format = "%-I:%M %p"
#
# [[applets.clock.timezones]]
# name = "Tokyo"
# timezone = "Asia/Tokyo"
# format = "%H:%M"
hide_all_day_events = false
show_week_numbers = false

[applets.display]
# Manages display brightness.
label_format = ""
tooltip_format = "{source}: {percent}%"
scroll_step = 10

[applets.keyboard]

[applets.keyboard.labels]
# "English (US)" = "EN"
# "German" = "DE"

[applets.mpris]
label_format = "{artist} - {title}"
tooltip_format = "{player}: {artist} - {title}"
hide_when_empty = true
max_rows = 12
show_artwork = true
# filter_regex = [
#   "(?i)^firefox$",
#   "(?i)podcast",
# ]

[applets.network]
label_format = ""
tooltip_format = "{state}"

[applets.next_event]
label_format = "{name} {remaining}"
tooltip_format = "{name} ({time}) — {duration}"
threshold_minutes = 30

[applets.notifications]
label_format = ""
tooltip_format = "{count} notifications"
badge_style = "dot"
popup_timeout_ms = 5000
popup_visible_limit = 8
popup_position = "top_right"
popup_margin_x = 12
popup_margin_y = 12
max_history = 100
# filter_regex = [
#   "(?i)^discord$",
#   "(?i)build succeeded",
# ]
#
# [[applets.notifications.urgency_remap]]
# app_pattern = "(?i)^slack$"
# urgency = "critical"
#
# [[applets.notifications.urgency_remap]]
# app_pattern = "(?i)^telegram"
# urgency = "low"

[applets.pager]
display = "windows"
appearance = "dots"
active_workspace_label = "{index}"
inactive_workspace_label = "{index}"

[applets.printing]
display = "auto"

[applets.privacy]

[applets.removable]
show_when_empty = false
label_format = ""
tooltip_format = "{count} removable device(s), {mounted} mounted"

[applets.session]
label_format = "{user}"
tooltip_format = "{user} on {host}"
show_lock = true
show_logout = true
show_suspend = true
show_hibernate = false
show_reboot = true
show_shutdown = true
confirm_logout = true
confirm_suspend = true
confirm_hibernate = true
confirm_reboot = true
confirm_shutdown = true

[applets.weather]
city_name = ""
hourly_slots = 5
forecast_days = 5
label_format = "{temp}"
tooltip_format = "{condition} · {temp} · feels like {feels_like} · {location}"
refresh_interval = 1800
```

Read [Theming](./theming.md) for visual customization, [Applets](./applets/) for per-applet options, [Calendar Sources](./calendar.md) for event feeds, [Wallpaper](./wallpaper.md) for background settings, and [Lock](./lock.md) for lock screen settings.
