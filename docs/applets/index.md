# Applets

Applets are the small things in your panel: clock, battery, network, media, weather, custom launchers, and status widgets.

Place applets in a panel section:

```toml
[[panels]]
left = ["pager", "mpris"]
center = ["clock"]
right = ["network", "battery", "session"]
```

Configure an applet under `[applets.name]`:

```toml
[applets.clock]
label_format = "%H:%M"
tooltip_format = "%A, %B %-d"
```

To create a second copy or a custom-named applet, use `extends`:

```toml
[applets.short-battery]
extends = "battery"
label_on_battery = "{percentage}%"

[[panels]]
right = ["short-battery", "session"]
```

## Common Format Fields

Many applets support `label_format` and `tooltip_format`. Each applet section lists the formatting fields it supports.

An empty label means the applet shows only its icon.

## Built-In Applets

| Applet | What it shows |
|---|---|
| [`audio`](#audio) | Volume, mute state, output device, muted microphone indicator. |
| [`battery`](#battery) | Battery state, charge, time left, power profile. |
| [`bluetooth`](#bluetooth) | Bluetooth state and connected devices. |
| [`brightness`](#brightness) | Screen brightness with scroll control. |
| [`clipboard`](#clipboard) | Clipboard history. |
| [`clock`](#clock) | Time, date, calendar, and optional world clocks. |
| [`command`](#command) | A button or menu that runs commands. |
| [`exec`](#exec) | A live custom status widget from your script. |
| [`idle`](#idle-inhibitor) | System idle inhibitors and a manual "keep awake" toggle. |
| [`keyboard`](#keyboard) | Current keyboard layout. |
| [`mpris`](#mpris) | Media players and playback controls. |
| [`network`](#network) | Wi-Fi, wired network, and VPN status. |
| [`notifications`](#notifications) | Notification center and popups. |
| [`pager`](#pager) | Workspaces and windows. |
| [`privacy`](#privacy) | Camera, microphone, screen sharing, and location indicators. |
| [`removable`](#removable) | USB drives and removable storage. |
| [`session`](#session) | Lock, logout, suspend, restart, and shutdown. |
| [`tray`](#tray) | App tray icons. |
| [`weather`](#weather) | Current weather and forecast. |

## Audio

Shows the current output volume. Scroll on the applet to change volume.

```toml
[applets.audio]
label_format = "{volume}%"
tooltip_format = "{device} - {volume}%"
scroll_step = 5
max_volume = 120
show_mic_indicator = true
show_streams = true
```

| Option | Default | Meaning |
|---|---|---|
| `show_icon` | `true` | Show the volume icon. |
| `show_mic_indicator` | `true` | Show a muted microphone indicator when the default input is muted. |
| `label_format` | `""` | Panel text. |
| `tooltip_format` | `"{device} - {volume}%"` | Hover text. |
| `scroll_step` | `10` | Volume change per scroll step. |
| `max_volume` | `100` | Maximum volume allowed from scrolling and controls. |
| `show_streams` | `true` | Show application streams in the popover. |

Placeholders: `{state}`, `{volume}`, `{device}`, `{input_volume}`, `{input_device}`.

## Battery

Shows battery and charging state. The popover includes time left and power information when available.

```toml
[applets.battery]
show_icon = true
label_on_battery = "{percentage}%"
label_on_ac = ""
tooltip_on_battery = "{percentage}% {state}, {time_left}"
tooltip_on_ac = "{percentage}% {state}"
settings_command = "gnome-control-center power"
```

| Option | Default | Meaning |
|---|---|---|
| `show_icon` | `true` | Show the battery icon. |
| `label_on_battery` | `""` | Panel text while unplugged. |
| `label_on_ac` | `""` | Panel text while plugged in. |
| `tooltip_on_battery` | `"{percentage}% {state}, {time_left}"` | Hover text while unplugged. |
| `tooltip_on_ac` | `"{percentage}% {state}"` | Hover text while plugged in. |
| `settings_command` | `""` | Optional command for opening power settings. |

Placeholders: `{percentage}`, `{state}`, `{time_left}`.

## Bluetooth

Shows Bluetooth status and connected device count.

```toml
[applets.bluetooth]
label_format = "{devices}"
tooltip_format = "{devices} connected devices"
```

| Option | Default | Meaning |
|---|---|---|
| `label_format` | `""` | Panel text. |
| `tooltip_format` | `"{devices} connected devices"` | Hover text. |

Placeholders: `{devices}`, `{state}`.

## Brightness

Shows brightness. Scroll on the applet to change brightness. On multi-monitor
setups, scrolling adjusts the display attached to the panel first, then the
focused monitor, then the primary brightness source. Display brightness is
clamped to at least 1%.

```toml
[applets.brightness]
label_format = "{percent}%"
tooltip_format = "{source}: {percent}%"
scroll_step = 5
```

| Option | Default | Meaning |
|---|---|---|
| `label_format` | `""` | Panel text. |
| `tooltip_format` | `"{source}: {percent}%"` | Hover text. |
| `scroll_step` | `10` | Brightness change per scroll step. |

Placeholders: `{source}`, `{percent}`.

## Clipboard

Shows clipboard history and opens a popover for copying older entries.

```toml
[applets.clipboard]
label_format = "{count}"
tooltip_format = "{count} clipboard items"
show_when_empty = false
```

| Option | Default | Meaning |
|---|---|---|
| `label_format` | `""` | Panel text. |
| `tooltip_format` | `"{count} clipboard items"` | Hover text. |
| `show_when_empty` | `false` | Keep the applet visible with no history. |

Placeholders: `{count}`, `{state}`.

## Clock

Shows time and opens a calendar popover.

```toml
[applets.clock]
label_format = "%H:%M"
tooltip_format = "%A, %B %-d %Y"
tick_interval = 1

[[applets.clock.timezones]]
name = "Tokyo"
timezone = "Asia/Tokyo"
format = "%H:%M"
```

| Option | Default | Meaning |
|---|---|---|
| `label_format` | `"%a %-d %b, %H:%M"` | Panel time format. |
| `tooltip_format` | `"%A, %-d %B %Y"` | Hover date format. |
| `tick_interval` | `1` | Seconds between clock updates. Values are clamped from 1 to 60. |
| `timezones` | `[]` | Optional world clocks shown in the popover. |

Clock formats use `strftime` style patterns.

### World clocks

Add one `[[applets.clock.timezones]]` table per extra timezone. World clocks appear in the clock popover, below the local calendar.

| Field | Default | Meaning |
|---|---|---|
| `name` | `""` | Display name. When empty, the timezone name is shown. |
| `timezone` | `"UTC"` | IANA timezone name, such as `"Europe/Warsaw"` or `"Asia/Tokyo"`. Invalid names are skipped. |
| `format` | `"%H:%M"` | Time format for this world clock row. |

```toml
[[applets.clock.timezones]]
name = "New York"
timezone = "America/New_York"
format = "%-I:%M %p"

[[applets.clock.timezones]]
name = "Tokyo"
timezone = "Asia/Tokyo"
format = "%H:%M"
```

### Common time formats

Copy these into `label_format`, `tooltip_format`, or a world clock `format`.

| Format | Example output | Notes |
|---|---|---|
| `"%H:%M"` | `23:07` | 24-hour time. |
| `"%H:%M:%S"` | `23:07:42` | 24-hour time with seconds. |
| `"%-I:%M %p"` | `11:07 PM` | 12-hour time without leading zero. |
| `"%a %-d %b, %H:%M"` | `Mon 11 May, 23:07` | Matches the default panel label shape. |
| `"%A, %-d %B %Y"` | `Monday, 11 May 2026` | Matches the default tooltip shape. |
| `"%Y-%m-%d %H:%M"` | `2026-05-11 23:07` | Sortable date and time. |

## Command

Runs a command from a panel button or menu.

```toml
# ~/.config/glimpse/applets/terminal.toml
id = "terminal"
type = "command"

[command]
icon = "utilities-terminal-symbolic"
label = "Terminal"
tooltip = "Open terminal"
command = ["ghostty"]
```

| Option | Default | Meaning |
|---|---|---|
| `type` | required | Use `"command"` in an applet package file. |
| `icon` | unset | Symbolic icon name. |
| `label` | unset | Optional button text. |
| `tooltip` | unset | Hover text. |
| `command` | `[]` | Command run when clicked. |
| `menu` | `[]` | Optional menu items with `label` and `command`. |

Read the full [Command Applet](../custom-applets/command.md) guide for menus and shell examples.

## Exec

Runs your own script and lets it draw status items and popovers.

```toml
# ~/.config/glimpse/applets/sysinfo.toml
id = "sysinfo"
type = "exec"

[exec]
command = ["sh", "-c", "~/.config/glimpse/scripts/sysinfo"]
restart_delay_ms = 1000
env_clear = false

[exec.env]
PATH = "/usr/bin:/bin"

[exec.options]
interval = 5
```

| Option | Default | Meaning |
|---|---|---|
| `type` | required | Use `"exec"` in an applet package file. |
| `command` | `[]` | Script or program to run. Required. |
| `restart_delay_ms` | `1000` | Delay before restarting the script after it exits. Minimum 50. |
| `options` | `{}` | Custom data sent to your script on startup. |
| `env_clear` | `false` | Clear the inherited environment before starting the script. |
| `env` | `{}` | Extra environment variables for the script. Applied after `env_clear`. |

Read [Exec Applet](../custom-applets/exec.md) for config and options, [Line Protocol](../custom-applets/exec-protocol.md) for raw protocol details, [Components](../custom-applets/exec-components.md) for popover component fields, and [Exec SDK](./exec-sdk.md) for SDK installation and language examples.

## Idle Inhibitor

Surfaces every active idle inhibitor on the session and lets you release the ones you own. The popover toggle is your manual "keep awake" — flipping it on holds both an `idle` and a `sleep` lock at the system level, so the screen won't blank *and* the machine won't auto-suspend.

The panel icon (an eye) flips with the toggle: closed when your manual hold is off, open when it's on. External inhibitors are listed in the popover but don't affect the panel icon — otherwise systems with persistent background inhibitors (e.g. `niri`'s power-key handler) would never see the icon change.

Four kinds of records appear in the popover:

| Source | Where it comes from | Releasable from popover |
|---|---|---|
| Your manual hold | Toggle on `Idle Inhibitor` popover | yes |
| `org.freedesktop.ScreenSaver` | Apps like Firefox, mpv, VLC, OBS, Steam | yes |
| `org.freedesktop.impl.portal.Inhibit` | Flatpak / sandboxed apps via xdg-desktop-portal | yes |
| `systemd-logind` | `systemd-inhibit`, package managers during upgrade, backup tools | no — we don't own the fd |

Row UX is icon + app name. Click a releasable row (or right-click) to open a one-item menu with **Release** — releases the inhibit and removes the row. Read-only `logind` rows have no menu and clicking them does nothing. Targets (`idle`, `suspend`, `shutdown`, lid/key handlers) and the full `who`/`why` strings sit in each row's tooltip.

```toml
[[panels]]
right = ["...", "idle"]
```

No per-instance configuration. The applet is added to the default right panel between `battery` and `session`; remove `"idle"` from the panel section if you don't want it visible.

### Daemon-side surfaces

`glimpse-idle` hosts three D-Bus services on the session bus:

| Bus name | Path | Purpose |
|---|---|---|
| `org.freedesktop.ScreenSaver` | `/ScreenSaver`, `/org/freedesktop/ScreenSaver` | Public interface for external `Inhibit`/`UnInhibit` callers |
| `me.aresa.GlimpseIdle.Portal` | `/org/freedesktop/portal/desktop` | xdg-desktop-portal backend for `org.freedesktop.impl.portal.Inhibit` |
| `me.aresa.GlimpseIdle` | `/me/aresa/GlimpseIdle/Inhibitors` | Glimpse-private read + admin-release API the panel proxies through |

If `org.freedesktop.ScreenSaver` is already owned by another process (`gsd-power` on GNOME, `kscreensaver` on KDE), our acquisition gracefully degrades — external screensaver-style apps will go to that other process, and only inhibitors arriving through the portal backend or logind will appear in the popover. The shell's own manual hold always reaches us via `me.aresa.GlimpseIdle` directly, regardless of who owns the public name.

### Installing the portal backend

The daemon ships an xdg-desktop-portal `.portal` file at `/usr/share/xdg-desktop-portal/portals/glimpse.portal` and a D-Bus session activation file. On the Glimpse desktop (`XDG_CURRENT_DESKTOP=glimpse`) the portal auto-routes to us. On other desktops, add the route in your `portals.conf`:

```ini
[preferred]
org.freedesktop.impl.portal.Inhibit=glimpse;
```

## Keyboard

Shows the current keyboard layout.

```toml
[keyboard]
remember = "global"

[keyboard.labels]
"English (US)" = "EN"
"Polish" = "PL"
```

| Setting | Default | Meaning |
|---|---|---|
| `[keyboard].remember` | `"global"` | How Glimpse remembers the active keyboard layout. |
| `[keyboard.labels]` | `{}` | Replace long layout names with short labels. |

### Remember modes

| Value | Behavior |
|---|---|
| `"global"` | Use one active layout everywhere. |
| `"app"` | Restore the previous layout when focus returns to the same application. |
| `"window"` | Restore the previous layout when focus returns to the same window. |

Set labels globally under `[keyboard.labels]`. Applet-local `labels` are still accepted as a compatibility fallback when `[keyboard.labels]` is empty.

## MPRIS

Shows media player status and playback controls.

```toml
[applets.mpris]
label_format = "{artist} - {title}"
tooltip_format = "{player}: {artist} - {title}"
hide_when_empty = true
max_rows = 5
show_artwork = true
```

| Option | Default | Meaning |
|---|---|---|
| `label_format` | `"{artist} - {title}"` | Panel text. |
| `tooltip_format` | `"{player}: {artist} - {title}"` | Hover text. |
| `hide_when_empty` | `true` | Hide when no player is active. |
| `max_rows` | `5` | Maximum players shown in the popover. Clamped from 1 to 12. |
| `show_artwork` | `true` | Show album art when available. |

Placeholders: `{player}`, `{artist}`, `{title}`, `{track}`, `{album}`, `{state}`, `{position}`, `{duration}`, `{remaining}`.

## Network

Shows current network state and opens a popover for Wi-Fi, wired network, and VPN entries.

```toml
[applets.network]
label_format = "{network}"
tooltip_format = "{state}"
```

| Option | Default | Meaning |
|---|---|---|
| `label_format` | `""` | Panel text. |
| `tooltip_format` | `"{state}"` | Hover text. |

Placeholders: `{state}`, `{network}`, `{type}`, `{wifi}`, `{access_points}`, `{connections}`, `{vpns}`, `{speed}`.

## Notifications

Shows notification state, a notification center, and popups.

```toml
[applets.notifications]
label_format = "{count}"
tooltip_format = "{count} notifications"
badge_style = "count"
max_history = 100
popup_timeout_ms = 5000
popup_visible_limit = 8
popup_position = "top_center"
popup_margin_x = 12
popup_margin_y = 32
popup_monitor = "DP-2"   # optional
filter_regex = [
  "(?i)^discord$",
  "(?i)build succeeded",
]
```

| Option | Default | Meaning |
|---|---|---|
| `label_format` | `""` | Panel text. |
| `tooltip_format` | `"{count} notifications"` | Hover text. |
| `badge_style` | `"count"` | Badge style. Use `"none"` to hide the badge. |
| `max_history` | `100` | Maximum notifications kept in memory. When the limit is exceeded, the oldest non-critical notifications are evicted first. Set to `0` for unlimited. |
| `popup_timeout_ms` | `5000` | How long popups stay visible. |
| `popup_visible_limit` | `8` | Maximum popups visible at once. Clamped from 1 to 20. |
| `popup_position` | `"top_center"` | Popup position: `"top_left"`, `"top_center"`, `"top_right"`, `"bottom_left"`, `"bottom_center"`, or `"bottom_right"`. |
| `popup_margin_x` | `12` | Horizontal popup margin. |
| `popup_margin_y` | `32` | Vertical popup margin. |
| `popup_monitor` | unset | Pin popups to a specific output by connector name (e.g. `"eDP-1"`, `"DP-2"`). When unset, Glimpse chooses a default monitor automatically. |
| `filter_regex` | `[]` | Regex rules matched against app name, title, and body. If any rule matches, non-critical notifications are hidden from popups, history, badge count, and the notification center. Critical notifications are never filtered. |

Placeholders: `{count}`, `{state}`.

### Popups on multi-monitor setups

By default Glimpse runs one panel per connected monitor, and the `notifications` applet on each of those panels would otherwise create its own popup window. To keep popups single, only one applet across the shell actually owns the popup window:

- If `popup_monitor` is set, only the applet whose panel sits on that connector owns the popup. Popups always appear on that monitor regardless of which monitor currently has focus.
- If `popup_monitor` is unset, Glimpse chooses a default monitor automatically.
- If the configured `popup_monitor` is unavailable, Glimpse cannot pin popups to that output. Notifications are still available from the notifications applet popover.

The `notifications` applet should be configured on at most one `[[panels]]` block — duplicates on the same connector are silently dropped (first one to initialize wins), but it's clearer to leave it in one place.

### Popup interactions

| Gesture | Behavior |
|---|---|
| Left-click on the card body | Focus the source application and dismiss the notification. |
| Right-click on the card body | Hide the popup card only; the notification stays in the notification center. |
| Close button (×) | Dismiss the notification fully. |
| Action button | Invoke that action and dismiss the notification. |

## Pager

Shows workspaces and windows.

```toml
[applets.pager]
display = "workspaces"
appearance = "numbers"
active_workspace_label = "{name}"
inactive_workspace_label = "{index}"
```

| Option | Default | Meaning |
|---|---|---|
| `display` | `"windows"` | Indicator and scroll target: `"workspaces"` or `"windows"`. |
| `appearance` | `"dots"` | Indicator style: `"dots"` or `"numbers"`. |
| `active_workspace_label` | `"{index}"` | Label format for the current workspace when `appearance = "numbers"`. |
| `inactive_workspace_label` | `"{index}"` | Label format for other workspaces when `appearance = "numbers"`. |

Workspace label placeholders: `{index}`, `{id}`, `{name}`. Unnamed workspaces render `{name}` as an empty string.

## Privacy

Shows privacy indicators such as microphone, camera, screen sharing, and location use.

The privacy applet has no config; add or remove `privacy` from a panel to show or hide it.

## Removable

Shows USB drives and removable storage.

```toml
[applets.removable]
show_when_empty = false
label_format = "{mounted}/{count}"
tooltip_format = "{count} removable device(s), {mounted} mounted"
```

| Option | Default | Meaning |
|---|---|---|
| `show_when_empty` | `false` | Keep visible with no removable devices. |
| `label_format` | `""` | Panel text. |
| `tooltip_format` | `"{count} removable device(s), {mounted} mounted"` | Hover text. |

Placeholders: `{count}`, `{mounted}`.

## Session

Shows the current user and opens session actions.

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

| Option | Default | Meaning |
|---|---|---|
| `label_format` | `"{user}"` | Panel text. |
| `tooltip_format` | `"{user} on {host}"` | Hover text. |
| `show_lock` | `true` | Show lock action. |
| `show_logout` | `true` | Show logout action. |
| `show_suspend` | `true` | Show suspend action. |
| `show_hibernate` | `false` | Show hibernate action. |
| `show_reboot` | `true` | Show restart action. |
| `show_shutdown` | `true` | Show shutdown action. |
| `confirm_logout` | `true` | Ask before logout. |
| `confirm_suspend` | `true` | Ask before suspend. |
| `confirm_hibernate` | `true` | Ask before hibernate. |
| `confirm_reboot` | `true` | Ask before restart. |
| `confirm_shutdown` | `true` | Ask before shutdown. |

Placeholders: `{user}`, `{host}`, `{uptime}`, `{state}`.

## Tray

Shows app tray icons.

```toml
[applets.tray]
icon_size = 16
show_passive = false
```

| Option | Default | Meaning |
|---|---|---|
| `icon_size` | `16` | Tray icon size. Clamped from 12 to 32. |
| `show_passive` | `false` | Show passive tray items. |

## Weather

Shows current weather and a forecast popover.

```toml
[applets.weather]
city_name = "Warsaw, PL"
hourly_slots = 5
forecast_days = 5
label_format = "{temp}"
tooltip_format = "{condition} · {temp} · feels like {feels_like} · {location}"
refresh_interval = 1800
```

| Option | Default | Meaning |
|---|---|---|
| `city_name` | `""` | Place to fetch. When empty, weather uses the shared `[location]` settings. |
| `hourly_slots` | `5` | Hourly entries in the popover. Clamped from 1 to 8. |
| `forecast_days` | `5` | Forecast days in the popover. Clamped from 1 to 10. |
| `label_format` | `"{temp}"` | Panel text. |
| `tooltip_format` | `"{condition} · {temp} · feels like {feels_like} · {location}"` | Hover text. |
| `refresh_interval` | `1800` | Seconds between updates. Minimum 60. |

Placeholders: `{temp}`, `{condition}`, `{feels_like}`, `{location}`.

To use your shared location, configure `[location]` and leave `city_name` empty:

```toml
[location]
provider = "static"
latitude = 52.2297
longitude = 21.0122

[applets.weather]
city_name = ""
```
