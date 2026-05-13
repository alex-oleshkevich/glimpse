# Exec Applet — Full Reference

The `exec` applet runs a long-running child process. The child controls the
applet's panel status item and popover content over a line-based JSON protocol
on stdin and stdout. Use it for custom widgets that need live state, custom
controls, or reactions to user input.

This page is self-contained. You do not need to read any other page to write a
working exec applet — config, raw protocol, every component, every event, and
SDK starters in four languages are all here.

---

## Table Of Contents

1. [Quickstart](#quickstart)
2. [Configuration](#configuration)
3. [Line Protocol](#line-protocol)
4. [Messages From Glimpse To Child](#messages-from-glimpse-to-child)
5. [Messages From Child To Glimpse](#messages-from-child-to-glimpse)
6. [Component Reference](#component-reference)
7. [Event Reference](#event-reference)
8. [Lifecycle And Restart Semantics](#lifecycle-and-restart-semantics)
9. [Best Practices](#best-practices)
10. [Raw Shell Starter](#raw-shell-starter)
11. [Python SDK Starter](#python-sdk-starter)
12. [TypeScript SDK Starter](#typescript-sdk-starter)
13. [Rust SDK Starter](#rust-sdk-starter)
14. [Go SDK Starter](#go-sdk-starter)

---

## Quickstart

The shortest possible exec applet is a shell script that prints one status
line. Save it as `~/.config/glimpse/scripts/hello`:

```sh
#!/bin/sh
printf 'status {"items":[{"id":"hello","label":"hi","icon":{"name":"face-smile-symbolic"}}]}\n'
exec sleep infinity
```

Then wire it up in `~/.config/glimpse/config.toml`:

```toml
[applets.hello]
extends = "exec"
command = ["sh", "-c", "~/.config/glimpse/scripts/hello"]

[[panels]]
right = ["hello"]
```

That's a complete, working exec applet. To go beyond, read the rest of this
page.

---

## Configuration

```toml
[applets.sysinfo]
extends = "exec"
command = ["sh", "-c", "~/.config/glimpse/scripts/sysinfo"]
restart_delay_ms = 1000
env_clear = false

[applets.sysinfo.env]
PATH = "/usr/bin:/bin"
LANG = "C.UTF-8"

[applets.sysinfo.options]
interval = 5
unit = "celsius"
```

| Option | Type | Default | Meaning |
|---|---|---|---|
| `extends` | string | required | Must be `"exec"`. |
| `command` | array of strings | required | Argv to spawn the child process. No shell expansion — wrap with `["sh", "-c", "..."]` for shell features. |
| `restart_delay_ms` | int | `1000` | Delay before restarting a crashed/exited child. Minimum `50`. |
| `env_clear` | bool | `false` | If `true`, the child's environment starts empty (only `env` entries are kept). |
| `env` | table of strings | `{}` | Extra env vars set on the child. Applied after `env_clear`. |
| `options` | TOML table | `{}` | Arbitrary per-instance configuration. Glimpse does not interpret it; it is forwarded verbatim in the first `init` line. Use it for polling intervals, units, thresholds, feature flags. |

The applet `<name>` is the instance identifier and is sent to the child as
`instance` in the `init` line.

---

## Line Protocol

Glimpse and the child exchange messages over the child's stdin/stdout. Each
message is **one line**: a command word, a single space, a JSON object,
and a newline.

```
command {"field":"value"}
```

Specifics:
- Each line **must** end in `\n` and the child **must** flush after each write.
- Bytes between newlines that do not match `^[a-z_]+ \{.*\}$` are ignored and
  logged. Bad JSON is also ignored.
- Unknown commands are ignored.
- The order of messages matters only insofar as last write wins per channel
  (`status` replaces previous status, `popover` replaces previous popover).
- The child should print its initial `status` immediately on startup; if it
  has popover content, it should also print `popover` at least once or when
  the popover lifecycle event arrives.
- stderr is free for diagnostics. Avoid noisy stderr; Glimpse logs it but
  does not surface it in the UI.

### Direction Summary

| Direction | Command | Purpose |
|---|---|---|
| Glimpse → child | `init` | One-time startup announcement with instance name + options. |
| Glimpse → child | `event` | User interaction or popover lifecycle. |
| Child → Glimpse | `status` | Replace panel status items. |
| Child → Glimpse | `popover` | Replace the popover content tree. |

---

## Messages From Glimpse To Child

### `init`

Sent exactly once, immediately after the child starts. Always precedes any
`event` line.

```
init {"instance":"sysinfo","options":{"interval":5,"unit":"celsius"}}
```

| Field | Type | Meaning |
|---|---|---|
| `instance` | string | The applet `<name>` from the config. |
| `options` | object | Verbatim copy of `[applets.<name>.options]`. Empty `{}` if omitted in config. |

### `event`

Sent when the user interacts with an interactive status item or a popover
component, or when the popover opens/closes.

```
event {"id":"submit","type":"click","source":"popover","button":"left"}
event {"id":"volume","type":"change","source":"popover","value":0.72}
event {"id":"popover","type":"open","source":"popover"}
event {"id":"popover","type":"close","source":"popover"}
```

Common fields:

| Field | Type | Meaning |
|---|---|---|
| `id` | string | The `id` of the component that fired the event. For popover lifecycle, always `"popover"`. |
| `type` | string | One of `click`, `scroll`, `input`, `change`, `toggle`, `open`, `close`. |
| `source` | string | `"status"` for status-item events, `"popover"` for popover events. |

Type-specific fields are listed under [Event Reference](#event-reference).

---

## Messages From Child To Glimpse

### `status`

Replaces the entire set of status items for this applet. Send a complete
list every time — partial updates are not supported.

```
status {"items":[
  {"id":"cpu","icon":{"name":"cpu-symbolic"},"label":"42%","tooltip":"CPU"},
  {"id":"mem","icon":{"name":"memory-symbolic"},"label":"51%","tooltip":"Memory"}
]}
```

Each item:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `id` | string | unset | Required if the item should emit `click`/`scroll` events. |
| `icon` | object | unset | `{"name":"icon-name-symbolic"}` or `{"path":"/absolute/path.png"}`. |
| `label` | string | unset | Short text shown beside the icon. |
| `tooltip` | string | unset | Hover text. |

Left-click on a status item opens the popover (if the applet has any).
Right-click opens the item's context menu (if any was set on the corresponding
popover `item` — see component reference). Status items themselves do not
carry menus directly.

### `popover`

Replaces the popover content tree. Sent on first render and on every change
to the tree. Glimpse only re-renders the popover when it is open or about to
open — the child should still send updates whenever its model changes; Glimpse
will buffer them.

```
popover {"root":{"type":"section","data":{
  "title":"System",
  "body":[
    {"type":"item","data":{"label":"CPU","right":{"type":"badge","data":{"label":"42%"}}}},
    {"type":"meter","data":{"label":"Memory","value":0.51,"text":"51%"}}
  ]
}}}
```

Top-level shape:

| Field | Meaning |
|---|---|
| `root` | A single component node (the full popover tree). May be `null` to clear. |

Every component node has the same envelope:

```json
{"type":"<component-name>","data":{ /* component-specific fields */ }}
```

---

## Component Reference

Every component accepts these **common fields** in its `data` object. They
default to unset unless noted.

| Field | Type | Values | Meaning |
|---|---|---|---|
| `id` | string | — | Required for interactive components (button, switch, scale, checkbox, dropdown, clickable item, interactive meter). Used as the `id` in events. |
| `visible` | bool | `true`/`false`; default `true` | Hide the component without removing it. |
| `hexpand` | bool | default `false` | Let the component grow horizontally. |
| `vexpand` | bool | default `false` | Let the component grow vertically. |
| `halign` | string | `fill`, `start`, `end`, `center`, `baseline` | Horizontal alignment. |
| `valign` | string | same | Vertical alignment. |
| `tooltip` | string | — | Hover text. |
| `variant` | string | `normal`, `muted`, `accent`, `success`, `warning`, `danger` | Visual emphasis. Default `normal`. |

The component-specific fields below are all in addition to these.

### Layout Components

#### `box`

Explicit horizontal-or-vertical layout container.

```json
{"type":"box","data":{
  "orientation":"vertical",
  "spacing":8,
  "children":[ /* nodes */ ]
}}
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `orientation` | string | `vertical` | `vertical` or `horizontal`. |
| `spacing` | int | `0` | Pixel gap between children. |
| `children` | array | `[]` | Child nodes. |

#### `row`

Horizontal layout (sugar for `box` with `orientation: horizontal`).

```json
{"type":"row","data":{"spacing":8,"children":[ /* nodes */ ]}}
```

| Field | Type | Default |
|---|---|---|
| `spacing` | int | `0` |
| `children` | array | `[]` |

#### `column`

Vertical layout (sugar for `box` with `orientation: vertical`).

```json
{"type":"column","data":{"spacing":8,"children":[ /* nodes */ ]}}
```

| Field | Type | Default |
|---|---|---|
| `spacing` | int | `0` |
| `children` | array | `[]` |

#### `grid`

Two-dimensional layout.

```json
{"type":"grid","data":{
  "row_spacing":4,
  "column_spacing":4,
  "children":[
    {"row":0,"column":0,"width":1,"height":1,"child":{"type":"label","data":{"text":"CPU"}}},
    {"row":0,"column":1,"width":1,"height":1,"child":{"type":"badge","data":{"label":"42%"}}}
  ]
}}
```

| Field | Type | Default |
|---|---|---|
| `row_spacing` | int | `0` |
| `column_spacing` | int | `0` |
| `children` | array of grid-children | `[]` |

Grid child shape:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `row` | int | `0` | Row index. |
| `column` | int | `0` | Column index. |
| `width` | int | `1` | Column span. |
| `height` | int | `1` | Row span. |
| `child` | node | required | The contained component. |

#### `scroll`

Wrap a node in a scrollable region.

```json
{"type":"scroll","data":{"child":{"type":"column","data":{"children":[ /* many nodes */ ]}}}}
```

| Field | Type | Meaning |
|---|---|---|
| `child` | node | Required. The content to scroll. |

#### `section`

Titled group with optional subtitle.

```json
{"type":"section","data":{
  "header":{"title":"Network","subtitle":"Connected"},
  "body":[ /* nodes */ ]
}}
```

| Field | Type | Default |
|---|---|---|
| `header` | object `{title, subtitle?}` | unset |
| `body` | array of nodes | `[]` |

Some SDKs also accept `title`/`subtitle` directly as a shorthand for the header.

#### `collapsible`

Section that can be expanded/collapsed by the user.

```json
{"type":"collapsible","data":{
  "header":{"title":"Advanced"},
  "expanded":false,
  "body":[ /* nodes */ ]
}}
```

| Field | Type | Default |
|---|---|---|
| `header` | object `{title, subtitle?}` | unset |
| `expanded` | bool | `false` |
| `body` | array of nodes | `[]` |

#### `card`

A framed group with no header.

```json
{"type":"card","data":{"children":[ /* nodes */ ]}}
```

| Field | Type | Default |
|---|---|---|
| `children` | array of nodes | `[]` |

#### `separator`

Visual divider.

```json
{"type":"separator","data":{"orientation":"horizontal"}}
```

| Field | Type | Default |
|---|---|---|
| `orientation` | string | unset (auto) |

### Display Components

#### `hero`

Large header at the top of a popover. Place it as the first child of a
column/section.

```json
{"type":"hero","data":{
  "title":"VPN",
  "subtitle":"Connected to wg0",
  "icon":{"name":"network-vpn-symbolic"}
}}
```

| Field | Type | Default |
|---|---|---|
| `title` | string | required |
| `subtitle` | string | `""` |
| `icon` | object | unset |

#### `label`

Plain text.

```json
{"type":"label","data":{"text":"CPU usage","wrap":false,"selectable":false}}
```

| Field | Type | Default |
|---|---|---|
| `text` | string | required |
| `wrap` | bool | `false` |
| `xalign` | float `0.0`–`1.0` | unset |
| `selectable` | bool | `false` |

#### `icon`

Symbolic icon rendered as a tree node.

```json
{"type":"icon","data":{"icon":{"name":"network-wireless-symbolic"},"pixel_size":24}}
```

| Field | Type | Default |
|---|---|---|
| `icon` | object | required (`{"name":...}` or `{"path":...}`) |
| `pixel_size` | int | unset |

#### `image`

Image from icon name or file path. Same shape as `icon`.

```json
{"type":"image","data":{"icon":{"path":"/home/me/.cache/avatar.png"},"pixel_size":64}}
```

#### `badge`

Small inline pill, typically used in `item.right` or as a status indicator.

```json
{"type":"badge","data":{"label":"42%","variant":"success"}}
```

| Field | Type | Default |
|---|---|---|
| `label` | string | required |

#### `status`

Small status marker (a colored dot). Use `variant` to color it.

```json
{"type":"status","data":{"variant":"success"}}
```

No specific fields beyond the common ones.

#### `meter`

Progress row with label and value. Can be made interactive (slider behavior).

```json
{"type":"meter","data":{
  "icon":{"name":"audio-volume-medium-symbolic"},
  "label":"Volume",
  "value":0.42,
  "min":0.0,
  "max":1.0,
  "step":0.01,
  "text":"42%",
  "interactive":false
}}
```

| Field | Type | Default |
|---|---|---|
| `icon` | object | unset |
| `label` | string | `""` |
| `value` | float | required |
| `min` | float | `0.0` |
| `max` | float | `1.0` |
| `step` | float | `0.01` |
| `text` | string | unset (defaults to a formatted percent) |
| `interactive` | bool | `false` |

When `interactive: true`, dragging emits a `change` event with the new `value`.

#### `progress`

Plain progress bar.

```json
{"type":"progress","data":{"value":0.7,"max":1.0,"show_text":true,"text":"70%"}}
```

| Field | Type | Default |
|---|---|---|
| `value` | float | required |
| `max` | float | `1.0` |
| `show_text` | bool | `false` |
| `text` | string | unset |

#### `spinner`

Loading indicator.

```json
{"type":"spinner","data":{"spinning":true}}
```

| Field | Type | Default |
|---|---|---|
| `spinning` | bool | `true` |

#### `copyable`

Label + value pair with a copy-to-clipboard affordance on the value.

```json
{"type":"copyable","data":{"label":"IPv4","value":"10.0.0.42"}}
```

| Field | Type | Default |
|---|---|---|
| `label` | string | `""` |
| `value` | string | required |

#### `toast`

Inline notice / alert.

```json
{"type":"toast","data":{
  "icon":{"name":"dialog-warning-symbolic"},
  "title":"Update available",
  "message":"glimpse 0.8.0 is available.",
  "action":{"id":"update","label":"Update"}
}}
```

| Field | Type | Default |
|---|---|---|
| `icon` | object | unset |
| `title` | string | required |
| `message` | string | `""` |
| `action` | object `{id, label}` | unset |

If `action` is set, clicking it emits a `click` event with the action's `id`.

#### `empty_state`

Friendly placeholder when there's nothing to show.

```json
{"type":"empty_state","data":{"title":"No devices","subtitle":"Plug in a USB device to start."}}
```

| Field | Type | Default |
|---|---|---|
| `title` | string | required |
| `subtitle` | string | `""` |

### List & Item Components

#### `item`

Standard list row. Left content, label, right content, optional context menu.

```json
{"type":"item","data":{
  "id":"wifi-home",
  "left":{"type":"icon","data":{"icon":{"name":"network-wireless-symbolic"}}},
  "label":"home-5G",
  "right":{"type":"badge","data":{"label":"−42 dBm"}},
  "clickable":true,
  "menu":[
    {"id":"forget","label":"Forget","enabled":true,"visible":true},
    {"id":"details","label":"Details"}
  ]
}}
```

| Field | Type | Default |
|---|---|---|
| `left` | node | unset |
| `label` | string | `""` |
| `right` | node | unset |
| `clickable` | bool | `false` |
| `menu` | array of menu items | `[]` |

Each `menu` entry:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `id` | string | required | Event id when the entry is selected. |
| `label` | string | required | Display text. |
| `enabled` | bool | `true` | Greyed out when `false`. |
| `visible` | bool | `true` | Hidden when `false`. |

#### `collapsible_item`

Item that expands to reveal nested content.

```json
{"type":"collapsible_item","data":{
  "left":{"type":"icon","data":{"icon":{"name":"folder-symbolic"}}},
  "label":"Devices",
  "expanded":false,
  "body":[ /* nodes */ ]
}}
```

| Field | Type | Default |
|---|---|---|
| `left` | node | unset |
| `label` | string | `""` |
| `right` | node | unset |
| `expanded` | bool | `false` |
| `body` | array of nodes | `[]` |

#### `action_row`

Larger summary-style row, usually for top-level actions inside a section.

```json
{"type":"action_row","data":{
  "id":"connect",
  "title":"Connect to VPN",
  "subtitle":"wg0",
  "meta":"4 routes",
  "icon":{"name":"network-vpn-symbolic"}
}}
```

| Field | Type | Default |
|---|---|---|
| `title` | string | required |
| `subtitle` | string | `""` |
| `meta` | string | `""` |
| `icon` | object | unset |

Emits `click` events when the row's `id` is set.

#### `action_menu`

Compact picker — a list of mutually selectable / toggleable actions.

```json
{"type":"action_menu","data":{
  "header":"Power profile",
  "items":[
    {"id":"power-saver","label":"Power Saver","checked":false},
    {"id":"balanced","label":"Balanced","checked":true},
    {"id":"performance","label":"Performance","checked":false}
  ]
}}
```

| Field | Type | Default |
|---|---|---|
| `header` | string | unset |
| `items` | array of action-menu items | `[]` |

Each item:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `id` | string | required | Event id. |
| `label` | string | required | Display text. |
| `icon` | object | unset | Optional icon. |
| `visible` | bool | `true` | Hide without removing. |
| `checked` | bool | unset | Show a checkmark. |
| `selectable` | bool | `true` | If `false`, the item is decorative. |

Selecting an item emits a `click` event with that item's `id`.

#### `detail_grid`

Two-column key/value table.

```json
{"type":"detail_grid","data":{"rows":[
  {"key":"SSID","value":"home-5G"},
  {"key":"IPv4","value":"10.0.0.42"},
  {"key":"Gateway","value":"10.0.0.1"}
]}}
```

| Field | Type | Default |
|---|---|---|
| `rows` | array of `{key, value}` | `[]` |

### Interactive Controls

#### `button`

```json
{"type":"button","data":{"id":"deploy","label":"Deploy","icon":{"name":"rocket-symbolic"}}}
```

| Field | Type | Default |
|---|---|---|
| `id` | string | required for events |
| `label` | string | unset |
| `icon` | object | unset |
| `child` | node | unset (use for fully custom content) |

Emits `click` with `button: "left"`.

#### `switch`

```json
{"type":"switch","data":{"id":"vpn","label":"VPN","active":false}}
```

| Field | Type | Default |
|---|---|---|
| `id` | string | required |
| `label` | string | unset |
| `active` | bool | `false` |

Emits `toggle` with `active` (and equivalently `value`).

#### `checkbox`

```json
{"type":"checkbox","data":{"id":"autostart","label":"Run at login","active":true}}
```

Same fields and events as `switch`.

#### `scale`

Slider.

```json
{"type":"scale","data":{
  "id":"brightness",
  "min":0.0,
  "max":1.0,
  "step":0.05,
  "value":0.6,
  "orientation":"horizontal",
  "draw_value":true
}}
```

| Field | Type | Default |
|---|---|---|
| `id` | string | required |
| `min` | float | `0.0` |
| `max` | float | `1.0` |
| `step` | float | `0.1` |
| `value` | float | `0.0` |
| `orientation` | string | unset (horizontal) |
| `draw_value` | bool | `false` |

Emits `change` with numeric `value`.

#### `dropdown`

```json
{"type":"dropdown","data":{
  "id":"network",
  "selected":1,
  "items":[
    {"id":"home","label":"home-5G"},
    {"id":"office","label":"office"}
  ]
}}
```

| Field | Type | Default |
|---|---|---|
| `id` | string | required |
| `items` | array of `{id, label}` | `[]` |
| `selected` | int | unset |

Emits `change` with the selected item's `id` and index.

---

## Event Reference

All events have at minimum `id`, `type`, `source`. Type-specific fields:

| Event source | `type` | Extra fields | Emitted by |
|---|---|---|---|
| Status item | `click` | `button: "left"\|"middle"\|"right"` | `status` item with `id` |
| Status item | `scroll` | `delta_y: float` | `status` item with `id` |
| Popover button | `click` | `button: "left"` | `button` |
| Popover item | `click` | `button: "left"` | `item` with `clickable:true` and `id` |
| Popover action_row | `click` | `button: "left"` | `action_row` with `id` |
| Popover action_menu item | `click` | — (id is the item's id) | `action_menu` |
| Popover switch | `toggle` | `active: bool`, `value: bool` | `switch` |
| Popover checkbox | `toggle` | `active: bool`, `value: bool` | `checkbox` |
| Popover scale | `change` | `value: float` | `scale` |
| Popover interactive meter | `change` | `value: float` | `meter` with `interactive:true` |
| Popover dropdown | `change` | `value: {id, label, index}` | `dropdown` |
| Popover input | `input` | `text: string` | (reserved for future input components) |
| Popover lifecycle | `open` / `close` | `id: "popover"` | popover open/close |

Example wire forms:

```
event {"id":"cpu","type":"click","source":"status","button":"middle"}
event {"id":"cpu","type":"scroll","source":"status","delta_y":-1.0}
event {"id":"deploy","type":"click","source":"popover","button":"left"}
event {"id":"vpn","type":"toggle","source":"popover","active":true}
event {"id":"brightness","type":"change","source":"popover","value":0.7}
event {"id":"network","type":"change","source":"popover","value":{"id":"office","label":"office","index":1}}
event {"id":"popover","type":"open","source":"popover"}
event {"id":"popover","type":"close","source":"popover"}
```

---

## Lifecycle And Restart Semantics

- Glimpse spawns the child process on panel startup, or when the applet is
  hot-reloaded into a panel.
- The child receives `init` once, then any number of `event` lines over its
  lifetime.
- When the child exits (any cause: crash, normal exit, signal), Glimpse waits
  `restart_delay_ms` and respawns. Restarts are unbounded by default; if the
  child keeps crashing in a loop, the delay throttles the rate.
- A respawned child receives a fresh `init` with the current options. The
  previous state is lost — persist anything you need to keep across restarts
  in a file or external store.
- Glimpse closes the child's stdin when the applet is removed or the panel
  shuts down. The child should treat stdin EOF as a shutdown signal.
- The child should never write to stdout outside of valid protocol lines.
  Stray bytes corrupt nothing but waste log space.

---

## Best Practices

| Practice | Why |
|---|---|
| Print initial `status` immediately, before any blocking work. | The panel should not sit empty while the child warms up. |
| Send complete `status` and `popover` payloads every time. | Each message replaces the previous one. There are no diffs. |
| Keep status labels short (1–6 chars). | Long labels make the panel jump and crowd neighbors. |
| Put detail in the popover, glanceable summary in the panel. | The panel is for at-a-glance state; popovers carry explanations and controls. |
| Use stable component `id`s across renders. | Events become harder to reason about when ids drift. |
| Throttle polling (5–30 s for most stats). | Sub-second polling wastes CPU. |
| Use variants sparingly. | `warning`/`danger` should mean something needs attention. |
| Treat stdin EOF as shutdown. | The child should exit cleanly so Glimpse does not restart it during teardown. |
| Use `env_clear` + `env` when the child should not inherit the user env. | Reproducible behavior, smaller attack surface. |
| Log to stderr, not stdout. | stdout is the protocol channel. Anything non-protocol there is wasted. |
| Validate your JSON before publishing. | Bad JSON is silently dropped. |
| Prefer many small applets over one mega-script. | Easier to debug, easier to fail in isolation. |

---

## Raw Shell Starter

This is a complete, self-contained shell applet that drives a CPU-temperature
status item and shows a basic popover with one toggle button. No SDK is needed
— it's just `sh`, `printf`, and a loop.

```sh
#!/bin/sh
# ~/.config/glimpse/scripts/cpu-temp

set -eu

render_status() {
  printf 'status {"items":[{"id":"cpu","icon":{"name":"temperature-symbolic"},"label":"%s","tooltip":"CPU temperature"}]}\n' "$1"
}

render_popover() {
  printf 'popover {"root":{"type":"section","data":{"header":{"title":"CPU"},"body":[{"type":"item","data":{"label":"Temperature","right":{"type":"badge","data":{"label":"%s"}}}},{"type":"button","data":{"id":"refresh","label":"Refresh"}}]}}}\n' "$1"
}

read_temp() {
  sensors 2>/dev/null | awk '/Package id 0/ {print $4; exit}' | tr -d '+°C'
}

last=""

# Initial paint.
temp="$(read_temp)"
temp="${temp:-n/a}"
render_status "$temp"
render_popover "$temp"
last="$temp"

# Poll + react to events.
while true; do
  # Drain any pending events for 5 seconds.
  if IFS= read -r -t 5 line; then
    case "$line" in
      init\ *)
        : ;;
      event\ *)
        case "$line" in
          *'"id":"refresh"'*)
            temp="$(read_temp)"
            temp="${temp:-n/a}"
            render_status "$temp"
            render_popover "$temp"
            last="$temp"
            ;;
        esac
        ;;
    esac
  else
    temp="$(read_temp)"
    temp="${temp:-n/a}"
    if [ "$temp" != "$last" ]; then
      render_status "$temp"
      render_popover "$temp"
      last="$temp"
    fi
  fi
done
```

Config:

```toml
[applets.cpu-temp]
extends = "exec"
command = ["sh", "-c", "~/.config/glimpse/scripts/cpu-temp"]
restart_delay_ms = 1000
```

---

## Python SDK Starter

**Install:**

```sh
pip install glimpse-applet-sdk
```

**Applet:**

```python
from dataclasses import dataclass

from glimpse_sdk import (
    Applet,
    AppletState,
    Button,
    Column,
    Hero,
    Icon,
    Item,
    RenderResult,
    Section,
    StatusItem,
    click,
)


@dataclass
class CounterState(AppletState):
    count: int = 0


class CounterApplet(Applet[CounterState]):
    def initial_state(self) -> CounterState:
        return CounterState()

    async def render(self) -> RenderResult:
        return RenderResult(
            status=[
                StatusItem(
                    id="counter",
                    icon=Icon.name("view-refresh-symbolic"),
                    label=str(self.state.count),
                )
            ],
            tree=Column(
                spacing=8,
                children=[
                    Hero(
                        icon=Icon.name("view-refresh-symbolic"),
                        title="Counter",
                        subtitle=f"Value: {self.state.count}",
                    ),
                    Section(
                        title="Controls",
                        body=[
                            Item(label="Current", right=None),
                            Button(id="increment", label="Increment"),
                        ],
                    ),
                ],
            ),
        )

    @click("increment")
    async def on_increment(self, _event) -> None:
        await self.set_state(count=self.state.count + 1)


if __name__ == "__main__":
    CounterApplet().run()
```

Config:

```toml
[applets.counter]
extends = "exec"
command = ["python", "/home/me/applets/counter.py"]
```

**SDK essentials:**

- Subclass `Applet[YourState]` where `YourState` is a `@dataclass` extending
  `AppletState`.
- Implement `initial_state()` and `async render() -> RenderResult`.
- Register handlers with decorators: `@click(id)`, `@scroll(id)`,
  `@input(id)`, `@change(id)`, `@toggle(id)`, `@event(id, type)`.
- Mutate state with `await self.set_state(**fields)` — this triggers a
  re-render.
- The init payload is exposed via `self.options` (a dict) after `on_init`.
- Override `async on_init(event)` for one-shot setup; `event.options` is the
  options dict.

---

## TypeScript SDK Starter

**Install:**

```sh
npm install glimpse-sdk
```

**Applet:**

```ts
import {
  Applet,
  Button,
  Column,
  Hero,
  Icon,
  Item,
  RenderResult,
  Section,
  StatusItem,
} from "glimpse-sdk";

interface CounterState {
  count: number;
}

class CounterApplet extends Applet<CounterState> {
  protected initialState(): CounterState {
    return { count: 0 };
  }

  constructor() {
    super();
    this.onClick("increment", async () => {
      await this.setState({ count: this.state.count + 1 });
    });
  }

  protected async render(): Promise<RenderResult> {
    return new RenderResult({
      status: [
        new StatusItem({
          id: "counter",
          icon: Icon.name("view-refresh-symbolic"),
          label: String(this.state.count),
        }),
      ],
      tree: new Column({
        spacing: 8,
        children: [
          new Hero({
            icon: Icon.name("view-refresh-symbolic"),
            title: "Counter",
            subtitle: `Value: ${this.state.count}`,
          }),
          new Section({
            title: "Controls",
            body: [
              new Item({ label: "Current" }),
              new Button({ id: "increment", label: "Increment" }),
            ],
          }),
        ],
      }),
    });
  }
}

await new CounterApplet().run();
```

Config:

```toml
[applets.counter]
extends = "exec"
command = ["node", "/home/me/applets/counter.js"]
```

**SDK essentials:**

- Extend `Applet<YourState>` (state is a plain interface).
- Implement `initialState()` and `async render(): Promise<RenderResult>`.
- Register handlers in the constructor with `this.onClick(id, fn)`,
  `this.onScroll`, `this.onInput`, `this.onChange`, `this.onToggle`.
- Mutate state with `await this.setState({ ... })` — partial patch object.
- Override `async onInit(event)` for one-shot setup.

---

## Rust SDK Starter

**`Cargo.toml`:**

```toml
[dependencies]
async-trait = "0.1"
glimpse-sdk = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

**`src/main.rs`:**

```rust
use async_trait::async_trait;
use glimpse_sdk::{
    Applet, AppletResult, Button, CallbackEvent, Column, Hero, Icon, Item, RenderResult, Section,
    StateStore, StatusItem, TreeNode, run, tree,
};

#[derive(Debug, Clone, Default)]
struct CounterState {
    count: u32,
}

struct CounterApplet {
    store: StateStore<CounterState>,
}

#[async_trait]
impl Applet for CounterApplet {
    type State = CounterState;

    fn store(&self) -> &StateStore<Self::State> {
        &self.store
    }

    fn store_mut(&mut self) -> &mut StateStore<Self::State> {
        &mut self.store
    }

    async fn render(&self) -> AppletResult<RenderResult> {
        Ok(RenderResult {
            status: vec![StatusItem::new("counter")
                .icon(Icon::name("view-refresh-symbolic"))
                .label(self.state().count.to_string())],
            tree: Some(
                Column::new(tree![
                    Hero::new("Counter", format!("Value: {}", self.state().count))
                        .icon(Icon::name("view-refresh-symbolic")),
                    Section::new(
                        "Controls",
                        tree![
                            Item::new("Current"),
                            Button::new("increment").label("Increment"),
                        ],
                    ),
                ])
                .spacing(8)
                .into(),
            ),
        })
    }

    async fn on_callback(&mut self, event: CallbackEvent) -> AppletResult<()> {
        if let CallbackEvent::Click(click) = event {
            if click.id == "increment" {
                self.set_state(|state| state.count += 1);
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(CounterApplet {
        store: StateStore::new(CounterState::default()),
    })
    .await
}
```

Config:

```toml
[applets.counter]
extends = "exec"
command = ["/home/me/.cargo/bin/counter-applet"]
```

**SDK essentials:**

- Implement the `Applet` trait. The two required boilerplate accessors are
  `store()` / `store_mut()`.
- `render()` returns a `RenderResult { status, tree }`. Wrap any widget into
  `TreeNode` via `TreeNode::from(widget)` or `.into()`.
- Mutate state with `self.set_state(|s| { ... })` — a sync closure that takes
  `&mut Self::State`.
- Match incoming events in `on_callback(event)`.
- Hand the applet to `run(applet).await` from `main`.

---

## Go SDK Starter

**Install:**

```sh
go get github.com/alex-oleshkevich/glimpse/sdk/sdk-go
```

**`main.go`:**

```go
package main

import (
	"context"
	"fmt"

	sdk "github.com/alex-oleshkevich/glimpse/sdk/sdk-go/sdk"
)

type counterState struct {
	Count int
}

type counterApplet struct {
	sdk.BaseApplet[counterState]
}

func newCounterApplet() *counterApplet {
	return &counterApplet{
		BaseApplet: sdk.NewBaseApplet(counterState{}),
	}
}

func (a *counterApplet) OnStart(context.Context) error               { return nil }
func (a *counterApplet) OnInit(context.Context, sdk.InitEvent) error { return nil }

func (a *counterApplet) OnCallback(_ context.Context, event sdk.CallbackEvent) error {
	if click, ok := event.(sdk.ClickEvent); ok && click.ID == "increment" {
		a.SetState(func(state *counterState) { state.Count++ })
	}
	return nil
}

func (a *counterApplet) Render(context.Context) (sdk.RenderResult, error) {
	count := a.State().Count
	return sdk.RenderResult{
		Status: []sdk.StatusItem{{
			ID:    "counter",
			Icon:  sdk.IconName("view-refresh-symbolic"),
			Label: fmt.Sprintf("%d", count),
		}},
		Tree: sdk.Column{
			Spacing: 8,
			Children: []sdk.Widget{
				sdk.Hero{Title: "Counter", Subtitle: fmt.Sprintf("Value: %d", count)},
				sdk.Section{
					Header: &sdk.Header{Title: "Controls"},
					Body: []sdk.Widget{
						sdk.Item{Label: "Current"},
						sdk.Button{
							CommonProps: sdk.CommonProps{ID: "increment"},
							Label:       "Increment",
						},
					},
				},
			},
		},
	}, nil
}

func main() {
	if err := sdk.Run[counterState](context.Background(), newCounterApplet()); err != nil {
		panic(err)
	}
}
```

Config:

```toml
[applets.counter]
extends = "exec"
command = ["/home/me/applets/counter"]
```

**SDK essentials:**

- Embed `sdk.BaseApplet[YourState]` in your struct.
- Implement `OnStart`, `OnInit`, `OnCallback`, `Render`. The base provides
  `State()` and `SetState(func(*State))`.
- `Render` returns a `RenderResult { Status, Tree }`. `Tree` is any value
  implementing `sdk.Widget` (or `nil` for no popover).
- Compose trees with struct literals: `sdk.Hero{Title: "..."}`,
  `sdk.Column{Children: []sdk.Widget{...}}`, etc. Every widget type
  satisfies `sdk.Widget`.
- Call `sdk.Run[State](ctx, applet)` from `main`.
