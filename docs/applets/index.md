# Applets

Applets are the small panel items that show status, open popovers, or run quick actions. Most people only need two steps:

1. Put applets on a panel in `~/.config/glimpse/config.toml`.
2. Override the applet sections you care about.

```toml
[[panels]]
position = "top"
height = 34
start = ["pager", "mpris"]
center = ["clock"]
end = ["tray", "network", "audio", "battery", "notifications", "session"]

[applets.clock]
label_format = "%a %-d %b, %H:%M"

[applets.audio]
tooltip_format = "{device} - {volume}%"
```

For the full panel layout shape, see [Configuration](../configuration.md).

## Second Copies

To create a second copy of a built-in applet, give it your own name and point `extends` at the built-in type.

```toml
[applets.short-battery]
extends = "battery"
label_on_battery = "{percentage}%"

[[panels]]
end = ["short-battery", "session"]
```

## Format Strings

Many applets use format strings. Text is shown as-is, and placeholders are replaced at runtime.

```toml
[applets.battery]
label_on_battery = "{percentage}%"
tooltip_on_battery = "{state}, {time_left}"
```

If a placeholder is unknown, it is left unchanged.

## Built-In Applets

Use these names in a panel section:

| Applet | Purpose |
| --- | --- |
| `audio` | Volume, mute state, devices, and streams |
| `battery` | Battery level and charging state |
| `bluetooth` | Bluetooth power state and connected devices |
| `clipboard` | Clipboard history status |
| `clock` | Time, calendar popover, and optional calendar sources |
| `display` | Brightness status and brightness control |
| `idle` | Idle inhibitor status |
| `keyboard` | Keyboard layout indicator |
| `mpris` | Media player controls |
| `network` | Network state and connections |
| `next_event` | Upcoming calendar event |
| `notifications` | Notification history and popups |
| `pager` | Workspaces or windows |
| `privacy` | Camera, microphone, screen sharing, and location indicators |
| `printing` | Print job status |
| `removable` | Removable drives |
| `session` | User session actions |
| `tray` | Status notifier tray |
| `weather` | Current weather and forecast |

`command` and `exec` are package applets. They live in `~/.config/glimpse/applets` and are documented near the end of this page.

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

| Field | Default | Notes |
| --- | --- | --- |
| `show_icon` | `true` | Shows the output volume icon. |
| `show_muted_indicator` | `true` | Shows when input or output is muted. |
| `label_format` | `""` | Optional text next to the icon. |
| `tooltip_format` | `"{device} - {volume}%"` | Tooltip text. |
| `scroll_step` | `10` | Volume step for mouse wheel changes. |
| `max_volume` | `100` | Maximum volume set from the panel. |
| `show_streams` | `true` | Shows application streams in the popover. |

Placeholders: `{state}`, `{volume}`, `{device}`, `{input_volume}`, `{input_device}`.

## Battery

```toml
[applets.battery]
show_icon = true
label_on_battery = ""
label_on_ac = ""
tooltip_on_battery = "{percentage}% {state}, {time_left}"
tooltip_on_ac = "{percentage}% {state}"
```

| Field | Default | Notes |
| --- | --- | --- |
| `show_icon` | `true` | Shows the battery icon. |
| `label_on_battery` | `""` | Label while running on battery. |
| `label_on_ac` | `""` | Label while plugged in. |
| `tooltip_on_battery` | `"{percentage}% {state}, {time_left}"` | Tooltip while running on battery. |
| `tooltip_on_ac` | `"{percentage}% {state}"` | Tooltip while plugged in. |

Placeholders: `{percentage}`, `{state}`, `{time_left}`.

## Bluetooth

```toml
[applets.bluetooth]
label_format = ""
tooltip_format = "{devices} connected devices"
```

| Field | Default | Notes |
| --- | --- | --- |
| `label_format` | `""` | Optional text next to the icon. |
| `tooltip_format` | `"{devices} connected devices"` | Tooltip text. |

Placeholders: `{devices}`, `{state}`.

## Clipboard

```toml
[applets.clipboard]
label_format = ""
tooltip_format = "{count} clipboard items"
show_when_empty = false
```

| Field | Default | Notes |
| --- | --- | --- |
| `label_format` | `""` | Optional text next to the icon. |
| `tooltip_format` | `"{count} clipboard items"` | Tooltip text. |
| `show_when_empty` | `false` | Keeps the applet visible when history is empty. |

Placeholders: `{count}`, `{state}`.

## Clock

```toml
[applets.clock]
label_format = "%a %-d %b, %H:%M"
tooltip_format = "%A, %-d %B %Y"
hide_all_day_events = false
show_week_numbers = false

# Optional extra clocks in the popover.
# [[applets.clock.timezones]]
# name = "New York"
# timezone = "America/New_York"
# format = "%H:%M"
#
# [[applets.clock.timezones]]
# name = "Tokyo"
# timezone = "Asia/Tokyo"
# format = "%H:%M"
```

| Field | Default | Notes |
| --- | --- | --- |
| `label_format` | `"%a %-d %b, %H:%M"` | Panel label using `strftime`. |
| `tooltip_format` | `"%A, %-d %B %Y"` | Tooltip using `strftime`. |
| `timezones` | `[]` | Extra timezone rows in the popover. |
| `hide_all_day_events` | `false` | Hides all-day events in the calendar popover. |
| `show_week_numbers` | `false` | Shows ISO week numbers in the calendar popover. |

Timezone fields:

| Field | Default | Notes |
| --- | --- | --- |
| `name` | `""` | Display name in the popover. |
| `timezone` | `"UTC"` | IANA timezone name. |
| `format` | `"%H:%M"` | Time format for that row. |

Calendar sources are configured separately. See [Calendar Sources](../calendar.md).

## Display

The display applet shows brightness and can manage brightness from the panel.

```toml
[applets.display]
label_format = ""
tooltip_format = "{source}: {percent}%"
scroll_step = 10
```

| Field | Default | Notes |
| --- | --- | --- |
| `label_format` | `""` | Optional text next to the icon. |
| `tooltip_format` | `"{source}: {percent}%"` | Tooltip text. |
| `scroll_step` | `10` | Brightness step for mouse wheel changes. |

Placeholders: `{source}`, `{percent}`.

## Idle

```toml
# Add `idle` to a panel section.
[[panels]]
end = ["idle"]
```

The idle applet has no per-applet config. It reflects the current idle inhibition state and lets you toggle it from the panel.

## Keyboard

Keyboard layout memory is configured globally because the same active layout state is shared across panels. Display labels belong to the keyboard applet.

```toml
[keyboard]
remember = "window"

[applets.keyboard.labels]
# "English (US)" = "EN"
# "German" = "DE"
```

| Field | Default | Notes |
| --- | --- | --- |
| `remember` | `"window"` | Scope used to remember the selected layout. |

`remember` values:

| Value | Meaning |
| --- | --- |
| `window` | Remember layout per window. |
| `app` | Remember layout per application. |
| `global` | Use one layout everywhere. |

The panel item itself only needs the applet name:

```toml
[[panels]]
end = ["keyboard"]
```

## MPRIS

Think of MPRIS as the "Now Playing" applet. It shows the current player and track, opens playback controls, and can hide noisy players with regex filters.

MPRIS is the Linux desktop media-player interface used by apps such as Spotify, VLC, mpv, browsers, and music players.

```toml
[applets.mpris]
label_format = "{artist} - {title}"
tooltip_format = "{player}: {artist} - {title}"
hide_when_empty = true
max_rows = 12
show_artwork = true

# Optional filters. Each regex is matched against player identity,
# title, artist, album, and player id.
# filter_regex = ["(?i)firefox", "(?i)chromium"]
```

| Field | Default | Notes |
| --- | --- | --- |
| `label_format` | `"{artist} - {title}"` | Panel label. |
| `tooltip_format` | `"{player}: {artist} - {title}"` | Tooltip text. |
| `hide_when_empty` | `true` | Hides the applet when no player is active. |
| `max_rows` | `12` | Maximum player rows in the popover, clamped from `1` to `12`. |
| `show_artwork` | `true` | Shows track artwork when available. |
| `filter_regex` | `[]` | Regex filters for players to hide. |

Placeholders: `{player}`, `{artist}`, `{title}`, `{track}`, `{album}`, `{state}`, `{position}`, `{duration}`, `{remaining}`.

## Network

```toml
[applets.network]
label_format = ""
tooltip_format = "{state}"
```

| Field | Default | Notes |
| --- | --- | --- |
| `label_format` | `""` | Optional text next to the icon. |
| `tooltip_format` | `"{state}"` | Tooltip text. |

Placeholders: `{state}`, `{network}`, `{type}`, `{wifi}`, `{access_points}`, `{connections}`, `{vpns}`, `{speed}`.

## Next Event

```toml
[applets.next_event]
label_format = "{name} {remaining}"
tooltip_format = "{name} ({time}) - {duration}"
threshold_minutes = 30
```

| Field | Default | Notes |
| --- | --- | --- |
| `label_format` | `"{name} {remaining}"` | Panel label. |
| `tooltip_format` | `"{name} ({time}) - {duration}"` | Tooltip text. |
| `threshold_minutes` | `30` | Only shows events starting within this many minutes. Minimum is `1`. |

Placeholders: `{name}`, `{time}`, `{duration}`, `{source}`, `{remaining}`, `{location}`.

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

# Optional notification filters.
# filter_regex = ["(?i)spotify", "(?i)build succeeded"]
#
# Optional urgency remapping.
# [[applets.notifications.urgency_remap]]
# app_pattern = "(?i)^slack$"
# urgency = "critical"
```

| Field | Default | Notes |
| --- | --- | --- |
| `label_format` | `""` | Optional text next to the icon. |
| `tooltip_format` | `"{count} notifications"` | Tooltip text. |
| `badge_style` | `"dot"` | Badge style for unread notifications. |
| `popup_timeout_ms` | `5000` | Popup timeout in milliseconds. |
| `popup_visible_limit` | `8` | Maximum visible popups, clamped from `1` to `20`. |
| `popup_position` | `"top_right"` | Popup corner or edge position. |
| `popup_margin_x` | `12` | Horizontal popup margin in pixels. |
| `popup_margin_y` | `12` | Vertical popup margin in pixels. |
| `popup_monitor` | unset | Optional monitor name for popups. |
| `max_history` | `100` | Maximum stored notification history. Set `0` for unlimited. |
| `filter_regex` | `[]` | Regex filters for non-critical notifications to hide. Rules match app name, title, and body. |
| `urgency_remap` | `[]` | Rules that rewrite notification urgency by app name. |

`badge_style` values:

| Value | Meaning |
| --- | --- |
| `none` | No badge. |
| `count` | Show unread count. |
| `dot` | Show a small unread dot. |

`popup_position` values:

| Value |
| --- |
| `top_left` |
| `top_center` |
| `top_right` |
| `bottom_left` |
| `bottom_center` |
| `bottom_right` |

Urgency values:

| Value |
| --- |
| `low` |
| `normal` |
| `critical` |

Placeholders: `{count}`, `{state}`.

## Pager

```toml
[applets.pager]
display = "windows"
appearance = "dots"
active_workspace_label = "{index}"
inactive_workspace_label = "{index}"
```

| Field | Default | Notes |
| --- | --- | --- |
| `display` | `"windows"` | What the pager represents. |
| `appearance` | `"dots"` | Visual style. |
| `active_workspace_label` | `"{index}"` | Label for the active workspace. |
| `inactive_workspace_label` | `"{index}"` | Label for inactive workspaces. |

`display` values:

| Value | Meaning |
| --- | --- |
| `windows` | Show open windows. |
| `workspaces` | Show workspaces. |

`appearance` values:

| Value | Meaning |
| --- | --- |
| `dots` | Compact dots. |
| `numbers` | Workspace numbers or labels. |

Workspace placeholders: `{index}`, `{id}`, `{name}`.

## Printing

```toml
[applets.printing]
display = "auto"
```

| Field | Default | Notes |
| --- | --- | --- |
| `display` | `"auto"` | Visibility mode for the panel item. |

`display` values:

| Value | Meaning |
| --- | --- |
| `auto` | Show when there are active jobs or printer errors. |
| `always` | Keep the applet visible even when idle. |

## Privacy

```toml
# Add `privacy` to a panel section.
[[panels]]
end = ["privacy"]
```

The privacy applet has no config. It shows privacy-sensitive activity such as microphone, camera, screen sharing, and location usage.

## Removable

```toml
[applets.removable]
show_when_empty = false
label_format = ""
tooltip_format = "{count} removable device(s), {mounted} mounted"
```

| Field | Default | Notes |
| --- | --- | --- |
| `show_when_empty` | `false` | Keeps the applet visible when no removable device is present. |
| `label_format` | `""` | Optional text next to the icon. |
| `tooltip_format` | `"{count} removable device(s), {mounted} mounted"` | Tooltip text. |

Placeholders: `{count}`, `{mounted}`.

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

| Field | Default | Notes |
| --- | --- | --- |
| `label_format` | `"{user}"` | Panel label. |
| `tooltip_format` | `"{user} on {host}"` | Tooltip text. |
| `show_lock` | `true` | Shows the lock action. |
| `show_logout` | `true` | Shows the logout action. |
| `show_suspend` | `true` | Shows the suspend action. |
| `show_hibernate` | `false` | Shows the hibernate action. |
| `show_reboot` | `true` | Shows the reboot action. |
| `show_shutdown` | `true` | Shows the shutdown action. |
| `confirm_logout` | `true` | Confirms before logout. |
| `confirm_suspend` | `true` | Confirms before suspend. |
| `confirm_hibernate` | `true` | Confirms before hibernate. |
| `confirm_reboot` | `true` | Confirms before reboot. |
| `confirm_shutdown` | `true` | Confirms before shutdown. |

Placeholders: `{user}`, `{host}`, `{uptime}`, `{state}`.

## Tray

```toml
[applets.tray]
icon_size = 16
show_passive = false
```

| Field | Default | Notes |
| --- | --- | --- |
| `icon_size` | `16` | Tray icon size, clamped from `12` to `32`. |
| `show_passive` | `false` | Shows passive tray items. |

## Weather

```toml
[applets.weather]
city_name = ""
hourly_slots = 5
forecast_days = 5
label_format = "{temp}"
tooltip_format = "{condition} · {temp} · feels like {feels_like} · {location}"
refresh_interval = 1800
```

| Field | Default | Notes |
| --- | --- | --- |
| `city_name` | `""` | City to show. Empty uses the shared location provider when available. |
| `hourly_slots` | `5` | Hourly forecast slots, clamped from `1` to `8`. |
| `forecast_days` | `5` | Forecast days, clamped from `1` to `10`. |
| `label_format` | `"{temp}"` | Panel label. |
| `tooltip_format` | `"{condition} · {temp} · feels like {feels_like} · {location}"` | Tooltip text. |
| `refresh_interval` | `1800` | Refresh interval in seconds. |

Placeholders: `{temp}`, `{condition}`, `{feels_like}`, `{location}`.

For automatic location, configure the shared location provider:

```toml
[location]
provider = "geo_clue"
```

## Package Applets

Package applets are small TOML files under `~/.config/glimpse/applets`. Use them when you want a custom command button or a custom process-driven applet.

### Command

A command applet is a button with optional click, scroll, and menu actions.

```toml
# ~/.config/glimpse/applets/terminal.toml
id = "terminal"
type = "command"

[command]
icon = "utilities-terminal-symbolic"
label = "Terminal"
tooltip = "Open terminal"
on_click = ["foot"]
on_middle_click = []
on_scroll_up = []
on_scroll_down = []
on_scroll_left = []
on_scroll_right = []

[[command.menu]]
label = "Files"
command = ["nautilus"]
```

| Field | Default | Notes |
| --- | --- | --- |
| `icon` | unset | Icon name. |
| `label` | unset | Button label. |
| `tooltip` | unset | Tooltip text. |
| `on_click` | `[]` | Command run on left click. |
| `on_middle_click` | `[]` | Command run on middle click. |
| `on_scroll_up` | `[]` | Command run on scroll up. |
| `on_scroll_down` | `[]` | Command run on scroll down. |
| `on_scroll_left` | `[]` | Command run on horizontal scroll left. |
| `on_scroll_right` | `[]` | Command run on horizontal scroll right. |
| `menu` | `[]` | Right-click menu items. |

Menu item fields:

| Field | Default | Notes |
| --- | --- | --- |
| `label` | `""` | Menu row label. |
| `command` | `[]` | Command run when the row is clicked. |

Add the package applet by its `id`:

```toml
[[panels]]
end = ["terminal"]
```

### Exec

An exec applet runs a long-lived program that sends widget updates to the panel. Use the SDKs for non-trivial applets. See [Custom Applets](../custom-applets/) and the [Exec SDK Reference](./exec-sdk.md).

```toml
# ~/.config/glimpse/applets/weather-line.toml
id = "weather-line"
type = "exec"

[exec]
command = ["python", "/home/alex/.config/glimpse/scripts/weather-line.py"]
restart_delay_ms = 1000
env_forward = false
env = {}

# Optional working directory for the child process.
# work_dir = "/home/alex/.config/glimpse/scripts"

[exec.options]
city = "Warsaw"
```

| Field | Default | Notes |
| --- | --- | --- |
| `command` | `[]` | Program and arguments. Required for a working applet. |
| `restart_delay_ms` | `1000` | Delay before restart after exit. Minimum is `50`. |
| `options` | `{}` | Applet-specific options passed to the child process. |
| `env_forward` | `false` | Set `true` to inherit the parent process environment. |
| `env` | `{}` | Extra environment variables for the child process. |
| `work_dir` | unset | Working directory for the child process. |

Add the package applet by its `id`:

```toml
[[panels]]
end = ["weather-line"]
```
