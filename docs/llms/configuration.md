# Configuration reference

This LLM-optimized reference covers the main Glimpse config file. Use it when generating or editing `~/.config/glimpse/config.toml`.

## File discovery

Glimpse reads the first config file found in this order:

| Priority | Path |
|---|---|
| 1 | `GLIMPSE_CONFIG` environment variable |
| 2 | `./config.toml` in the current working directory |
| 3 | `$XDG_CONFIG_HOME/glimpse/config.toml` |
| 4 | `$HOME/.config/glimpse/config.toml` when `XDG_CONFIG_HOME` is unset |

Custom command and exec applets are package files under `$XDG_CONFIG_HOME/glimpse/applets` or linked project applets created with applet tooling.

## Includes

The main config can include other TOML files with a top-level array:

```toml
include = ["common.toml", "laptop.toml"]
```

Included files load first, in list order. The file that declares `include` loads last and overrides included values. Relative include paths resolve relative to the file that declares them, not necessarily relative to the root `config.toml`. Included files are watched for hot reload.

Merge rules:

| Values | Result |
|---|---|
| table + table | Recursive merge |
| array + array | Later array replaces earlier array |
| scalar + scalar | Later scalar replaces earlier scalar |
| mixed types | Later value replaces earlier value |

Use includes for shared defaults. Keep machine-specific or final overrides in the main `config.toml`.

## Minimal panel

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

`"..."` keeps the default applets for that panel section in place.

## Theme

| Field | Default | Values | Meaning |
|---|---|---|---|
| `theme` | `"adwaita"` | Theme pack directory name | Selects a theme pack. |
| `theme_mode` | `"auto"` | `auto`, `dark`, `light` | Controls effective light/dark mode for CSS, wallpaper/backdrop/lock image variants, and color-scheme. |

`theme_mode = "auto"` follows solar day/night data when available. If solar data is unavailable, the panel keeps the previous effective mode; if none exists yet, it starts from light. Explicit `dark` or `light` ignores solar data.

A panel can override only its own mode:

```toml
[[panels]]
position = "top"
theme_mode = "dark"
left = ["pager"]
center = ["clock"]
right = ["network", "battery", "session"]
```

Panel-level `theme_mode` does not select a different theme pack.

## Panel fields

| Field | Default | Values | Meaning |
|---|---|---|---|
| `monitor` | unset | Output name | Pins the panel to one monitor. |
| `position` | `"top"` | `top`, `bottom`, `left`, `right` | Panel edge. |
| `size` | `36` | pixels | Panel thickness. |
| `margin` | all `0` | `{ left, right, top, bottom }` | Panel margins. |
| `theme_mode` | global mode | `auto`, `dark`, `light` | Per-panel light/dark override. |
| `left` | default left applets | applet ids | Left island. |
| `center` | default center applets | applet ids | Center island. |
| `right` | default right applets | applet ids | Right island. |

Reserved applet ids:

| Id | Meaning |
|---|---|
| `__dynamic__` | Dynamic island content managed by the shell. |
| `__dev__` | Applets started by `glimpse-shell applets dev`. |
| `...` | Preserve defaults for that panel section. |

## Built-in applet names

Use these ids in `left`, `center`, or `right`:

```text
audio, battery, bluetooth, brightness, clipboard, clock, command, display,
exec, idle, keyboard, mpris, network, next_event, notifications, pager,
printing, privacy, removable, session, tray, weather, window, workspace
```

Custom package applets use the package id from their `applet.toml`.

## Built-in applet config

Built-in applet overrides live under `[applets.<name>]`:

```toml
[applets.clock]
label_format = "%H:%M"

[applets.weather]
city_name = "Warsaw, PL"
label_format = "{temp}"
```

For every built-in applet field, see [Applet Configuration](./applets.md).

## Keyboard memory

Keyboard layout memory is global; display labels are applet-local:

```toml
[keyboard]
remember = "window"

[applets.keyboard.labels]
# "English (US)" = "EN"
# "German" = "DE"
```

`remember` values: `window`, `app`, `global`. Default is `window`.

## Location

Current shared location config supports GeoClue:

```toml
[location]
provider = "geo_clue"
```

Weather can bypass shared location with a fixed city:

```toml
[applets.weather]
city_name = "Warsaw, PL"
```

Static shared coordinates are not supported in the current config format.

## Night light

```toml
[night_light]
schedule = "automatic"
temperature = 4200
transition_minutes = 15
# start_time = "20:00"
# end_time = "06:00"
```

| Field | Default | Values | Meaning |
|---|---|---|---|
| `schedule` | `"automatic"` | `automatic`, `schedule`, `off` | Solar schedule, fixed clock schedule, or disabled gamma change. |
| `temperature` | `4200` | Kelvin | Lower values are warmer. |
| `transition_minutes` | `15` | minutes | Fade duration. |
| `start_time` | unset | `HH:MM` | Required for fixed schedule. |
| `end_time` | unset | `HH:MM` | Required for fixed schedule. |

Fixed schedule example:

```toml
[night_light]
schedule = "schedule"
start_time = "20:30"
end_time = "07:00"
temperature = 4200
transition_minutes = 15
```

## Idle

Default idle ladder:

| Step | AC | Battery |
|---|---|---|
| Monitor off | 600 seconds | 300 seconds |
| Lock session | 900 seconds | 900 seconds |
| Suspend | 3600 seconds | 1800 seconds |

```toml
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
```

Listener fields:

| Field | Required | Meaning |
|---|---|---|
| `timeout` | yes | Seconds of no input before firing. |
| `on_idle` | yes | Shell command run after timeout. |
| `on_resume` | no | Shell command run when activity returns after that listener fired. |
| `respect_inhibitors` | no | Overrides global inhibitor behavior for one listener. |

## Wallpaper, backdrop, and lock

```toml
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
```

## Calendar

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

Source types are `ical` and `directory`.

## Custom applet package

Package files live under `~/.config/glimpse/applets/`:

```toml
# ~/.config/glimpse/applets/terminal.toml
id = "terminal"
type = "command"

[command]
icon = "utilities-terminal-symbolic"
tooltip = "Open terminal"
command = ["ghostty"]
```

Then place `terminal` in a panel section.
