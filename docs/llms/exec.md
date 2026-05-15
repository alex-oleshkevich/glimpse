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
2. [Choosing Command Or Exec](#choosing-command-or-exec)
3. [Applet Project Directories](#applet-project-directories)
4. [`glimpse-applet` CLI Workflow](#glimpse-applet-cli-workflow)
5. [Configuration](#configuration)
6. [Line Protocol](#line-protocol)
7. [Messages From Glimpse To Child](#messages-from-glimpse-to-child)
8. [Messages From Child To Glimpse](#messages-from-child-to-glimpse)
9. [Component Reference](#component-reference)
10. [Event Reference](#event-reference)
11. [Lifecycle And Restart Semantics](#lifecycle-and-restart-semantics)
12. [Best Practices](#best-practices)
13. [Raw Shell Starter](#raw-shell-starter)
14. [Python SDK Starter](#python-sdk-starter)
15. [TypeScript SDK Starter](#typescript-sdk-starter)
16. [Rust SDK Starter](#rust-sdk-starter)
17. [Go SDK Starter](#go-sdk-starter)

---

## Quickstart

The shortest possible exec applet is a shell script that prints one status
line. Save it as `~/.config/glimpse/scripts/hello`:

```sh
#!/bin/sh
printf 'status {"items":[{"id":"hello","label":"hi","icon":{"name":"face-smile-symbolic"}}]}\n'
exec sleep infinity
```

Then create an applet package file and reference its id from panel config:

```toml
# ~/.config/glimpse/applets/hello.toml
id = "hello"
type = "exec"

[exec]
command = ["sh", "-c", "~/.config/glimpse/scripts/hello"]
```

```toml
# ~/.config/glimpse/config.toml
[[panels]]
right = ["hello"]
```

That's a complete, working exec applet. To go beyond, read the rest of this
page.

---

## Choosing Command Or Exec

Choose the applet type before writing code:

| Need | Applet type | Why |
|---|---|---|
| Launch an app or URL | `command` | A static button can run one configured command. |
| Show a small static menu of commands | `command` | Menu items are TOML config, not a child process. |
| Show changing status | `exec` | A child process can emit new `status` lines over time. |
| Show custom popover widgets | `exec` | The child process owns a component tree. |
| React to clicks, sliders, toggles, or popover open/close | `exec` | Glimpse sends `event` lines to the child. |
| Use Python, TypeScript, Rust, or Go SDK helpers | `exec` | SDKs wrap the exec protocol. |

LLM guidance: if the requested applet has state, live data, custom UI, or interactive controls, create an `exec` applet. Use `command` only for launchers and command menus.

---

## Applet Project Directories

Prefer project directories for custom applets. A project directory keeps the applet package, source code, and install metadata together.

```txt
counter/
  applet.toml
  main.py
```

The root file is always `applet.toml`. For an exec SDK applet it looks like:

```toml
id = "counter"
type = "exec"

[exec]
command = ["uv", "run", "main.py"]
```

For a command applet it looks like:

```toml
id = "terminal"
type = "command"

[command]
icon = "utilities-terminal-symbolic"
tooltip = "Open terminal"
command = ["ghostty"]
```

Project directories can be discovered in two ways:

| Method | File written | When to use |
|---|---|---|
| `glimpse-applet dev` | `~/.config/glimpse/applets/<id>.dev.toml` | Live development with rebuild and restart. |
| `glimpse-applet link` | `~/.config/glimpse/applets/<id>.toml` symlink | Normal use after the applet is ready. |

Add linked applets by id in a panel section:

```toml
[[panels]]
right = ["counter", "network", "battery"]
```

Add active dev applets with `__dev__`:

```toml
[[panels]]
right = ["network", "__dev__", "battery"]
```

If the same applet id appears in `~/.config/glimpse/config.toml` and in the discovered applets directory, the explicit `config.toml` entry wins. During development, remove or rename the explicit entry if it shadows the dev applet.

---

## `glimpse-applet` CLI Workflow

Use `glimpse-applet` to create, run, install, inspect, and debug applets.

### Create

```sh
glimpse-applet new counter --lang python
glimpse-applet new counter --lang typescript
glimpse-applet new counter --lang rust
glimpse-applet new counter --lang go
glimpse-applet new terminal --type command
```

`new` creates a project directory and writes `applet.toml`. Exec scaffolds include a starter SDK applet for the selected language.

### Develop

```sh
cd counter
glimpse-applet dev
```

`dev` behavior:

| Language | Build step | Runtime command | Watched paths |
|---|---|---|---|
| Rust | `cargo build --quiet` | `cargo run --quiet` | `src`, `Cargo.toml` |
| Python | none | `uv run main.py` | `main.py` |
| TypeScript | `npx tsc` | `node dist/main.js` | `src`, `tsconfig.json` |
| Go | `go build -o .dev-build` | `.dev-build` | project directory |

The dev command:

- writes `~/.config/glimpse/applets/<id>.dev.toml` while it runs;
- watches source files with a debounce window;
- rebuilds and restarts the child after source changes;
- forwards Glimpse stdin/stdout between the shell and applet;
- caches the `init` line and replays it to every restarted child;
- removes the generated `.dev.toml` file when the interactive dev process exits.

### Link And Remove

```sh
glimpse-applet link
glimpse-applet link /path/to/counter
glimpse-applet unlink
glimpse-applet unlink /path/to/counter
glimpse-applet rm counter
glimpse-applet rm counter --yes
```

`link` symlinks the project `applet.toml` into `~/.config/glimpse/applets/<id>.toml`. `unlink` removes that symlink. `rm` removes an installed applet file by id and asks for confirmation unless `--yes` is provided.

### Inspect And Diagnose

```sh
glimpse-applet list
glimpse-applet doctor
glimpse-applet doctor --lang python
glimpse-applet doctor --strict
```

`list` shows linked and active dev applets. `doctor` checks `glimpse-shell`, `glimpse-applet`, and language toolchains; `--strict` exits non-zero on failures.

### IPC Debugging

```sh
glimpse-applet watch
glimpse-applet watch bluetooth.*
glimpse-applet dispatch open_uri uri=https://example.com
```

`watch` subscribes to shell events and prints them. `dispatch` sends a shell IPC command and waits for an acknowledgement. Both commands use `GLIMPSE_IPC_SOCKET` if set, then `$XDG_RUNTIME_DIR/glimpse/ipc.sock`, then `/tmp/glimpse/ipc.sock`.

---

## Configuration

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
LANG = "C.UTF-8"

[exec.options]
interval = 5
unit = "celsius"
```

| Option | Type | Default | Meaning |
|---|---|---|---|
| `type` | string | required | Must be `"exec"`. |
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
| `instance` | string | The applet package `id`. |
| `options` | object | Verbatim copy of `[exec.options]`. Empty `{}` if omitted in the package file. |

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

### `popover`

Replaces the popover content tree. Sent on first render and on every change
to the tree. Glimpse only re-renders the popover when it is open or about to
open — the child should still send updates whenever its model changes; Glimpse
will buffer them.

```
popover {"root":{"type":"section","data":{
  "title":"System",
  "children":[
    {"type":"row","data":{"spacing":8,"children":[{"type":"label","data":{"text":"CPU"}},{"type":"badge","data":{"label":"42%"}}]}},
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
| `id` | string | — | Required for interactive components (button, switch, toggle_button, slider, checkbox, select, interactive meter). Used as the `id` in events. |
| `visible` | bool | `true`/`false`; default `true` | Hide the component without removing it. |
| `hexpand` | bool | default `false` | Let the component grow horizontally. |
| `vexpand` | bool | default `false` | Let the component grow vertically. |
| `halign` | string | `fill`, `start`, `end`, `center`, `baseline` | Horizontal alignment. |
| `valign` | string | same | Vertical alignment. |
| `tooltip` | string | — | Hover text. |
| `variant` | string | `normal`, `muted`, `accent`, `success`, `warning`, `danger` | Visual emphasis. Default `normal`. |

The component-specific fields below are all in addition to these.

Only the component names listed in this section are valid. Internal GTK component names such as `action_row` and `action_menu` are not exec protocol widget types; use `action_item`, `section`, `button`, `menu_button`, `row`, or `column` instead.

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

#### `overlay`

Stack render-only overlay nodes above a base child.

```json
{"type":"overlay","data":{"child":{"type":"picture","data":{"path":"/home/me/.cache/avatar.png","content_fit":"cover"}},"overlays":[{"type":"badge","data":{"label":"Live","halign":"end","valign":"start"}}]}}
```

| Field | Type | Default |
|---|---|---|
| `child` | node | required |
| `overlays` | array of nodes | `[]` |

#### `list_box`

GTK list with one row per child.

```json
{"type":"list_box","data":{"children":[{"type":"label","data":{"text":"First"}},{"type":"badge","data":{"label":"Second"}}]}}
```

| Field | Type | Default |
|---|---|---|
| `children` | array of nodes | `[]` |

#### `expander`

Collapsible disclosure container backed by `gtk::Expander`.

```json
{"type":"expander","data":{"label":"Details","expanded":true,"child":{"type":"label","data":{"text":"More"}}}}
```

| Field | Type | Default |
|---|---|---|
| `label` | string | required |
| `expanded` | bool | `false` |
| `child` | node | required |

#### `tree_expander`

GTK tree expander wrapper around one child.

```json
{"type":"tree_expander","data":{
  "child":{"type":"label","data":{"text":"Nested"}},
  "hide_expander":true,
  "indent_for_depth":true,
  "indent_for_icon":true
}}
```

| Field | Type | Default |
|---|---|---|
| `child` | node | required |
| `hide_expander` | bool | `false` |
| `indent_for_depth` | bool | `false` |
| `indent_for_icon` | bool | `false` |

#### `section`

Titled group with optional subtitle.

```json
{"type":"section","data":{
  "title":"Network",
  "subtitle":"Connected",
  "children":[ /* nodes */ ]
}}
```

| Field | Type | Default |
|---|---|---|
| `title` | string | `""` |
| `subtitle` | string | `""` |
| `children` | array of nodes | `[]` |

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

#### `picture`

Image file rendered with `gtk::Picture`.

```json
{"type":"picture","data":{"path":"/home/me/.cache/avatar.png","content_fit":"cover"}}
```

| Field | Type | Default |
|---|---|---|
| `path` | string file path | required |
| `content_fit` | string: `fill`, `contain`, `cover`, `scale_down` | `contain` |

#### `badge`

Small inline pill, typically used in rows, headers, or as a status indicator.

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

#### `level_bar`

Native GTK level indicator.

```json
{"type":"level_bar","data":{"value":0.7,"min":0.0,"max":1.0,"mode":"continuous"}}
```

| Field | Type | Default |
|---|---|---|
| `value` | float | required |
| `min` | float | `0.0` |
| `max` | float | `1.0` |
| `mode` | string: `continuous`, `discrete` | `continuous` |

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

#### `empty_state`

Friendly placeholder when there's nothing to show.

```json
{"type":"empty_state","data":{"title":"No devices","subtitle":"Plug in a USB device to start."}}
```

| Field | Type | Default |
|---|---|---|
| `title` | string | required |
| `subtitle` | string | `""` |

#### `property_list`

Two-column key/value table.

```json
{"type":"property_list","data":{"title":"Network","rows":[
  {"key":"SSID","value":"home-5G"},
  {"key":"IPv4","value":"10.0.0.42"},
  {"key":"Gateway","value":"10.0.0.1"}
]}}
```

| Field | Type | Default |
|---|---|---|
| `title` | string | `""` |
| `rows` | array of `{key, value}` | `[]` |

#### `item`

Display-only row with optional icon, sublabel, and a right child slot.

```json
{"type":"item","data":{
  "icon":"network-wireless-symbolic",
  "label":"Wi-Fi",
  "sublabel":"Connected",
  "right":{"type":"badge","data":{"label":"home-5G"}}
}}
```

| Field | Type | Default |
|---|---|---|
| `icon` | string icon name | `""` |
| `label` | string | required |
| `sublabel` | string | `""` |
| `right` | component node | unset |

#### `action_item`

Clickable row with optional icon, sublabel, and a render-only right child slot.

```json
{"type":"action_item","data":{
  "id":"wifi",
  "icon":"network-wireless-symbolic",
  "label":"Wi-Fi",
  "sublabel":"Connected",
  "right":{"type":"badge","data":{"label":"home-5G"}}
}}
```

| Field | Type | Default |
|---|---|---|
| `id` | string | required for events |
| `icon` | string icon name | `""` |
| `label` | string | required |
| `sublabel` | string | `""` |
| `right` | component node | unset |
| `enabled` | bool | `true` |

Emits `click` with `button: "left"` from the row. The `right` node is visual only.

### Interactive Controls

#### `button`

```json
{"type":"button","data":{"id":"deploy","label":"Deploy","icon":"rocket-symbolic","variant":"primary"}}
```

| Field | Type | Default |
|---|---|---|
| `id` | string | required for events |
| `label` | string | unset |
| `icon` | string | unset |
| `enabled` | bool | `true` |
| `variant` | string | `flat`; one of `primary`, `secondary`, `compact`, `flat`, `danger` |

Emits `click` with `button: "left"`.

#### `link_button`

Link-style button that opens a URI through GTK. It does not emit applet events.

```json
{"type":"link_button","data":{"uri":"https://example.com/docs","label":"Docs"}}
```

| Field | Type | Default |
|---|---|---|
| `uri` | string URI | required |
| `label` | string | unset |

#### `menu_button`

Button that opens a rendered nested popover. The `menu_button` itself does not emit applet events; controls inside its `popover` node keep their normal behavior.

```json
{"type":"menu_button","data":{
  "label":"More",
  "icon":"open-menu-symbolic",
  "popover":{"type":"label","data":{"text":"Menu content"}}
}}
```

| Field | Type | Default |
|---|---|---|
| `label` | string | unset |
| `icon` | string icon name | unset |
| `popover` | node | required |

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

#### `toggle_button`

Button-like toggle control.

```json
{"type":"toggle_button","data":{"id":"focus","label":"Focus mode","active":false}}
```

Same fields and events as `switch`.

#### `checkbox`

```json
{"type":"checkbox","data":{"id":"autostart","label":"Run at login","active":true}}
```

Same fields and events as `switch`.

#### `slider`

Slider.

```json
{"type":"slider","data":{
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

#### `select`

```json
{"type":"select","data":{
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

Emits `change` with the selected item's `id`, label, and index.

---

## Event Reference

All events have at minimum `id`, `type`, `source`. Type-specific fields:

| Event source | `type` | Extra fields | Emitted by |
|---|---|---|---|
| Status item | `click` | `button: "left"\|"middle"\|"right"` | `status` item with `id` |
| Status item | `scroll` | `delta_y: float` | `status` item with `id` |
| Popover button | `click` | `button: "left"` | `button` |
| Popover action item | `click` | `button: "left"` | `action_item` |
| Popover switch | `toggle` | `active: bool`, `value: bool` | `switch` |
| Popover toggle button | `toggle` | `active: bool`, `value: bool` | `toggle_button` |
| Popover checkbox | `toggle` | `active: bool`, `value: bool` | `checkbox` |
| Popover slider | `change` | `value: float` | `slider` |
| Popover interactive meter | `change` | `value: float` | `meter` with `interactive:true` |
| Popover select | `change` | `value: {id, label, index}` | `select` |
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
status item and shows a basic popover with one toggle button. No SDK is needed:
it uses `sh`, `printf`, and a loop.

```sh
#!/bin/sh
# ~/.config/glimpse/scripts/cpu-temp

set -eu

render_status() {
  printf 'status {"items":[{"id":"cpu","icon":{"name":"temperature-symbolic"},"label":"%s","tooltip":"CPU temperature"}]}\n' "$1"
}

render_popover() {
  printf 'popover {"root":{"type":"section","data":{"title":"CPU","children":[{"type":"row","data":{"spacing":8,"children":[{"type":"label","data":{"text":"Temperature"}},{"type":"badge","data":{"label":"%s"}}]}},{"type":"button","data":{"id":"refresh","label":"Refresh"}}]}}}\n' "$1"
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
id = "cpu-temp"
type = "exec"

[exec]
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
    ButtonVariant,
    Column,
    Hero,
    Icon,
    Label,
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

    async def status(self, state: CounterState):
        return [
            StatusItem(
                id="counter",
                icon=Icon.name("view-refresh-symbolic"),
                label=str(state.count),
            )
        ]

    async def popover(self, state: CounterState):
        return Column(
            spacing=8,
            children=[
                Hero(
                    icon=Icon.name("view-refresh-symbolic"),
                    title="Counter",
                    subtitle=f"Value: {state.count}",
                ),
                Section(
                    title="Controls",
                    children=[
                        Label(text="Current"),
                        Button(
                            id="increment",
                            label="Increment",
                            icon="list-add-symbolic",
                            variant=ButtonVariant.PRIMARY,
                        ),
                    ],
                ),
            ],
        )

    @click("increment")
    async def on_increment(self, _event) -> None:
        await self.set_state(count=self.state.count + 1)


if __name__ == "__main__":
    CounterApplet().run()
```

Config:

```toml
id = "counter"
type = "exec"

[exec]
command = ["python", "/home/me/applets/counter.py"]
```

**SDK essentials:**

- Subclass `Applet[YourState]` where `YourState` is a `@dataclass` extending
  `AppletState`.
- Implement `initial_state()`, plus `async status(state)` returning a list of
  `StatusItem` and `async popover(state)` returning a widget or `None`.
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
  Label,
  Section,
  StatusItem,
  type TreeNode,
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

  protected async status(state: CounterState): Promise<StatusItem[]> {
    return [
      new StatusItem({
        id: "counter",
        icon: Icon.name("view-refresh-symbolic"),
        label: String(state.count),
      }),
    ];
  }

  protected async popover(state: CounterState): Promise<TreeNode | null> {
    return new Column({
      spacing: 8,
      children: [
        new Hero({
          icon: Icon.name("view-refresh-symbolic"),
          title: "Counter",
          subtitle: `Value: ${state.count}`,
        }),
        new Section({
          title: "Controls",
          children: [
            new Label("Current"),
            new Button({
              id: "increment",
              label: "Increment",
              icon: "list-add-symbolic",
              variant: "primary",
            }),
          ],
        }),
      ],
    });
  }
}

await new CounterApplet().run();
```

Config:

```toml
id = "counter"
type = "exec"

[exec]
command = ["node", "/home/me/applets/counter.js"]
```

**SDK essentials:**

- Extend `Applet<YourState>` (state is a plain interface).
- Implement `initialState()`, plus `protected async status(state)` returning
  `StatusItem[]` and `protected async popover(state)` returning
  `TreeNode | null`.
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
glimpse-sdk = "0.3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

**`src/main.rs`:**

```rust
use async_trait::async_trait;
use glimpse_sdk::{
    Applet, AppletResult, Button, ButtonVariant, CallbackEvent, Column, Hero, Icon, Label, Section,
    StatusItem, TreeNode, run, tree,
};

#[derive(Debug, Clone, Default)]
struct CounterState {
    count: u32,
}

struct CounterApplet;

#[async_trait]
impl Applet for CounterApplet {
    type State = CounterState;

    async fn status(&self, state: &Self::State) -> AppletResult<Vec<StatusItem>> {
        Ok(vec![
            StatusItem::new("counter")
                .icon(Icon::name("view-refresh-symbolic"))
                .label(state.count.to_string()),
        ])
    }

    async fn popover(&self, state: &Self::State) -> AppletResult<Option<TreeNode>> {
        Ok(Some(
            Column::new(tree![
                Hero::new("Counter", format!("Value: {}", state.count))
                    .icon(Icon::name("view-refresh-symbolic")),
                Section::new(
                    "Controls",
                    tree![
                        Label::new("Current"),
                        Button::new("increment")
                            .label("Increment")
                            .icon("list-add-symbolic")
                            .variant(ButtonVariant::Primary),
                    ],
                ),
            ])
            .spacing(8)
            .into(),
        ))
    }

    async fn on_callback(
        &mut self,
        state: &mut Self::State,
        event: CallbackEvent,
    ) -> AppletResult<()> {
        if let CallbackEvent::Click(click) = event {
            if click.id == "increment" {
                state.count += 1;
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(CounterApplet, CounterState::default()).await
}
```

Config:

```toml
id = "counter"
type = "exec"

[exec]
command = ["/home/me/.cargo/bin/counter-applet"]
```

**SDK essentials:**

- Implement the `Applet` trait: `status(&state)` returning a `Vec<StatusItem>`
  and `popover(&state)` returning `Option<TreeNode>`. The applet struct itself
  is usually empty (`struct CounterApplet;`) since state lives in
  `Self::State` and is passed in by the runtime.
- Match incoming events in `on_callback(&mut state, event)`; mutate the
  passed-in `&mut state` directly.
- Wrap any widget into `TreeNode` via `TreeNode::from(widget)` or `.into()`,
  or use the `tree!` macro for collections.
- Hand the applet + initial state to `run(applet, state).await` from `main`.

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

func (a *counterApplet) Status(_ context.Context, state *counterState) ([]sdk.StatusItem, error) {
	return []sdk.StatusItem{{
		ID:    "counter",
		Icon:  sdk.IconName("view-refresh-symbolic"),
		Label: fmt.Sprintf("%d", state.Count),
	}}, nil
}

func (a *counterApplet) Popover(_ context.Context, state *counterState) (sdk.Widget, error) {
	return sdk.Column{
		Spacing: 8,
		Children: []sdk.Widget{
			sdk.Hero{Title: "Counter", Subtitle: fmt.Sprintf("Value: %d", state.Count)},
			sdk.Section{
				Title: "Controls",
				Children: []sdk.Widget{
					sdk.Label{Text: "Current"},
					sdk.Button{
						CommonProps: sdk.CommonProps{ID: "increment"},
						Label:       "Increment",
						Icon:        "list-add-symbolic",
						Variant:     sdk.ButtonVariantPrimary,
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
id = "counter"
type = "exec"

[exec]
command = ["/home/me/applets/counter"]
```

**SDK essentials:**

- Embed `sdk.BaseApplet[YourState]` in your struct.
- Implement `OnStart`, `OnInit`, `OnCallback`, plus
  `Status(ctx, *state) ([]sdk.StatusItem, error)` and
  `Popover(ctx, *state) (sdk.Widget, error)`. The base provides
  `State()` and `SetState(func(*State))`.
- `Popover` returns any value implementing `sdk.Widget`, or `nil` for no
  popover.
- Compose trees with struct literals: `sdk.Hero{Title: "..."}`,
  `sdk.Column{Children: []sdk.Widget{...}}`, etc. Every widget type
  satisfies `sdk.Widget`.
- Call `sdk.Run[State](ctx, applet)` from `main`.
