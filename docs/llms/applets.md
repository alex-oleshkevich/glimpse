# Applet configuration reference

This LLM-optimized reference covers built-in applet configuration in `~/.config/glimpse/config.toml`. Built-in applet overrides live under `[applets.<name>]`. Command and exec package applets live in separate `applet.toml` files under `~/.config/glimpse/applets`.

## Placement

```toml
[[panels]]
left = ["pager", "mpris", "__dev__"]
center = ["clock", "weather", "notifications", "privacy"]
right = ["__dynamic__", "tray", "network", "audio", "battery", "session"]
```

Built-in ids: `audio`, `battery`, `bluetooth`, `brightness`, `clipboard`, `clock`, `display`, `idle`, `keyboard`, `mpris`, `network`, `next_event`, `notifications`, `pager`, `printing`, `privacy`, `removable`, `session`, `tray`, `weather`, `window`, `workspace`.

Second copies use `extends`:

```toml
[applets.short-battery]
extends = "battery"
label_on_battery = "{percentage}%"
```

## Audio

```toml
[applets.audio]
show_icon = true
show_muted_indicator = true
label_format = ""
tooltip_format = "{device} - {volume}%"
scroll_step = 10
max_volume = 100
show_streams = true
```

Placeholders: `{state}`, `{volume}`, `{device}`, `{input_volume}`, `{input_device}`.

## Battery

```toml
[applets.battery]
show_icon = true
label_on_battery = ""
label_on_ac = ""
tooltip_on_battery = "{percentage}% {state}, {time_left}"
tooltip_on_ac = "{percentage}% {state}"
settings_command = ""
```

Placeholders: `{percentage}`, `{state}`, `{time_left}`. `settings_command` is accepted by config but not yet wired to a popover action.

## Bluetooth

```toml
[applets.bluetooth]
label_format = ""
tooltip_format = "{devices} connected devices"
```

Placeholders: `{devices}`, `{state}`.

## Brightness

The brightness applet controls screen brightness and shows a mouse-wheel-adjustable indicator.

```toml
[applets.brightness]
label_format = ""
tooltip_format = "{source}: {percent}%"
scroll_step = 10
```

Placeholders: `{source}`, `{percent}`.

## Clipboard

```toml
[applets.clipboard]
label_format = ""
tooltip_format = "{count} clipboard items"
show_when_empty = false
```

Placeholders: `{count}`, `{state}`.

## Clock

```toml
[applets.clock]
label_format = "%a %-d %b, %H:%M"
tooltip_format = "%A, %-d %B %Y"
tick_interval = 60
hide_all_day_events = false
show_week_numbers = false
# [[applets.clock.timezones]]
# name = "New York"
# timezone = "America/New_York"
# format = "%-I:%M %p"
#
# [[applets.clock.timezones]]
# name = "Tokyo"
# timezone = "Asia/Tokyo"
# format = "%H:%M"
```

Timezone fields: `name`, `timezone`, `format`. Calendar sources are configured globally under `[calendar]`.

## Display

The display applet shows connected monitors and lets you enable or disable them from the popover. It has no brightness control; use `brightness` for that.

```toml
[applets.display]
tooltip_format = "{active}/{total} monitors"
```

Placeholders: `{active}`, `{total}`.

## Idle

The idle applet has no per-applet config. Add `idle` to a panel section to show idle inhibitor state and a panel toggle.

## Keyboard

Keyboard layout memory is global; applet labels are local:

```toml
[keyboard]
remember = "window"

[applets.keyboard.labels]
# "English (US)" = "EN"
# "German" = "DE"
```

`remember` values: `window`, `app`, `global`. Default is `window`.

## MPRIS

MPRIS means Media Player Remote Interfacing Specification. In user-facing terms, this is the Now Playing applet.

```toml
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
```

Placeholders: `{player}`, `{artist}`, `{title}`, `{track}`, `{album}`, `{state}`, `{position}`, `{duration}`, `{remaining}`.

## Network

```toml
[applets.network]
label_format = ""
tooltip_format = "{state}"
```

Placeholders: `{state}`, `{network}`, `{type}`, `{wifi}` (alias for `{access_points}`), `{access_points}`, `{connections}`, `{vpns}`, `{speed}`.

## Next event

```toml
[applets.next_event]
label_format = "{name} {remaining}"
tooltip_format = "{name} ({time}) - {duration}"
threshold_minutes = 30
```

`threshold_minutes` minimum is `1`. Placeholders: `{name}`, `{time}`, `{duration}`, `{source}`, `{remaining}`, `{location}`.

## Notifications

```toml
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
```

Enums:

| Field | Values |
|---|---|
| `badge_style` | `dot`, `count`, `hidden` |
| `popup_position` | `top_left`, `top_center`, `top_right`, `bottom_left`, `bottom_center`, `bottom_right` |
| `urgency` | `low`, `normal`, `critical` |

Three or more notifications from the same application are grouped in the notification center. Click a collapsed group to expand it, or Shift-click any notification in the group to dismiss the whole group.

Placeholders: `{count}`, `{unread}`.

## Pager

```toml
[applets.pager]
display = "windows"
appearance = "dots"
active_workspace_label = "{index}"
inactive_workspace_label = "{index}"
```

Enums:

| Field | Values |
|---|---|
| `display` | `workspaces`, `windows` |
| `appearance` | `dots`, `numbers`, `names` |

Workspace label placeholders: `{index}`, `{name}`.

## Printing

```toml
[applets.printing]
display = "auto"
```

`display` values: `auto`, `always`, `never`.

## Privacy

The privacy applet has no per-applet config. Add `privacy` to show camera, microphone, screen sharing, and location indicators.

## Removable

```toml
[applets.removable]
show_when_empty = false
label_format = ""
tooltip_format = "{count} removable device(s), {mounted} mounted"
```

Placeholders: `{count}`, `{mounted}`, `{unmounted}`.

## Session

```toml
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
```

Placeholders: `{user}`, `{host}`.

## Tray

The tray applet has no per-applet config. Add `tray` to show StatusNotifier items.

## Weather

```toml
[applets.weather]
city_name = ""
geolocate = false
hourly_slots = 5
forecast_days = 5
label_format = "{temp}"
tooltip_format = "{condition} - {temp} - feels like {feels_like} - {location}"
refresh_interval = 1800
```

`city_name = ""` uses shared `[location]`. `geolocate` is accepted by config but not yet wired to weather refresh behavior. Placeholders: `{temp}`, `{feels_like}`, `{condition}`, `{location}`, `{humidity}`, `{wind}`.

## Window

Shows the title of the focused window. Hidden when no window is focused or the compositor does not support window tracking.

```toml
[applets.window]
label_format = "{title}"
max_chars = 80
# icon = "app"
```

`icon` is unset by default; `"app"` resolves the icon from the window's `.desktop` entry, any other string is used as a literal icon name. Placeholders: `{title}`, `{app_id}`, `{id}`, `{index}`.

## Workspace

Shows the name or index of the current workspace. Scroll switches workspaces; right-click opens a rename dialog on Niri and Hyprland.

```toml
[applets.workspace]
label_format = "{name_or_index}"
```

Placeholders: `{name_or_index}`, `{name}`, `{index}`, `{id}`.

## Package applets

Command applet package:

```toml
id = "terminal"
type = "command"

[command]
icon = "utilities-terminal-symbolic"
tooltip = "Open terminal"
command = ["ghostty"]
```

Exec applet package:

```toml
id = "weather-line"
type = "exec"

[exec]
command = ["python", "/path/to/weather-line.py"]
restart_delay_ms = 1000
env_forward = false
# work_dir = "/path/to/project"

[exec.options]
city = "Warsaw"
```

Command and exec applets are experimental; their package and SDK APIs may change.
