# Exec Line Protocol

The exec line protocol is a bidirectional stream over standard input and standard output. Glimpse starts the child process, sends setup and event lines to the child's stdin, and reads status and popover lines from the child's stdout.

Each protocol line starts with a command name, a space, and a JSON object:

```txt
command {"field":"value"}
```

| Direction | Command | Purpose |
|---|---|---|
| Child to Glimpse | `status` | Replaces the panel status items for this applet. |
| Child to Glimpse | `popover` | Replaces the popover content for this applet. |
| Glimpse to child | `init` | Announces the applet instance name and configured `options`. |
| Glimpse to child | `event` | Reports clicks, scrolls, popover opens/closes, and control changes. |

The normal flow is:

1. Glimpse starts the child process from `command`.
2. Glimpse sends one `init` line.
3. The child prints a `status` line, and optionally a `popover` line.
4. Glimpse sends `event` lines when the user interacts with status items or popover controls.
5. The child prints new `status` or `popover` lines whenever its state changes.

Unknown commands and invalid JSON are ignored and logged.

## Common Messages

| Message | Direction | Purpose | Payload shape |
|---|---|---|---|
| `init` | Glimpse to child | Startup data for this applet instance. | `{"instance":"name","options":{...}}` |
| `status` | Child to Glimpse | Current panel items. | `{"items":[...]}` |
| `popover` | Child to Glimpse | Current popover tree. | `{"root":{...}}` |
| `event` | Glimpse to child | User interaction or popover lifecycle event. | `{"id":"...","type":"...","source":"...",...}` |

Your child process sends:

```txt
status {"items":[{"id":"cpu","label":"42%","icon":{"name":"cpu-symbolic"},"tooltip":"CPU usage"}]}
popover {"root":{"type":"section","data":{"title":"System","body":[]}}}
```

Glimpse sends:

```txt
init {"instance":"sysinfo","options":{"interval":5,"unit":"celsius"}}
event {"id":"cpu","type":"click","source":"status","button":"left"}
```

## Status Messages

Status items are shown directly in the panel.

```txt
status {"items":[
  {"id":"cpu","icon":{"name":"cpu-symbolic"},"label":"12%","tooltip":"CPU"},
  {"id":"mem","icon":{"name":"memory-symbolic"},"label":"51%","tooltip":"Memory"}
]}
```

| Field | Default | Meaning |
|---|---|---|
| `id` | unset | Optional event id. Add it if you want clicks or scrolls. |
| `icon` | unset | Optional icon, either `{"name":"icon-name"}` or `{"path":"/path/to/image.png"}`. |
| `label` | unset | Optional text in the panel. |
| `tooltip` | unset | Optional hover text. |

Left-click opens the popover when the applet has popover content. Right-click opens the context menu if available.

## Popover Messages

A `popover` message replaces this applet's popover content. The payload has a `root` field containing a component tree.

```txt
popover {"root":{"type":"section","data":{
  "title":"System",
  "body":[
    {"type":"item","data":{"label":"CPU","right":{"type":"badge","data":{"label":"42%"}}}},
    {"type":"meter","data":{"label":"Memory","value":0.51,"text":"51%"}}
  ]
}}}
```

The `root` object uses this structure:

| Field | Meaning |
|---|---|
| `type` | Component name, such as `section`, `item`, `button`, or `meter`. |
| `data` | Component fields. The expected fields depend on `type`. |

Send a complete popover update whenever the content changes. Read [Components](./exec-components.md) for valid component types and fields.

## Interactive Events

Interactive status items and popover components send `event` lines back to the child process.

| Source | Event | Payload |
|---|---|---|
| Status item with `id` | `click`, `scroll` | `button` or `delta_y`. |
| Popover `button` | `click` | `button = "left"`. |
| Popover `item` with `clickable = true` and `id` | `click` | `button = "left"`. |
| Popover `action_menu` item | `click` | selected item id. |
| Popover `switch` | `toggle` | `active = true` or `false`. |
| Popover `checkbox` | `toggle` | `active = true` or `false`. |
| Popover `scale` | `change` | numeric `value`. |
| Interactive `meter` | `change` | numeric `value`. |
| Popover `dropdown` | `change` | selected item id, label, and index. |
| Popover lifecycle | `open`, `close` | id `popover`. |

Example event:

```txt
event {"id":"volume","type":"change","source":"popover","value":0.72}
```

## Shell Starter

```sh
#!/bin/sh

printf 'status {"items":[{"id":"load","label":"starting","icon":{"name":"utilities-system-monitor-symbolic"}}]}\n'

while IFS= read -r line; do
  case "$line" in
    init\ *)
      printf 'status {"items":[{"id":"load","label":"ready","icon":{"name":"utilities-system-monitor-symbolic"}}]}\n'
      ;;
    event\ *)
      printf 'popover {"root":{"type":"section","data":{"title":"System","body":[{"type":"item","data":{"label":"Last event","right":{"type":"badge","data":{"label":"seen"}}}}]}}}\n'
      ;;
  esac
done
```

This shape is event-driven. For polling, run a background loop and keep reading events in the foreground.

## How-To: CPU Temperature

```sh
#!/bin/sh

while true; do
  temp="$(sensors | rg 'Package id 0' | rg -o '[0-9]+\\.[0-9]+°C' | head -n1)"
  [ -n "$temp" ] || temp="n/a"
  printf 'status {"items":[{"id":"cpu","icon":{"name":"temperature-symbolic"},"label":"%s","tooltip":"CPU temperature"}]}\n' "$temp"
  sleep 5
done
```

Config:

```toml
[applets.cpu-temp]
extends = "exec"
command = ["sh", "-c", "~/.config/glimpse/scripts/cpu-temp"]
```

## How-To: Toggle A Command

Use a button and handle its click event:

```txt
popover {"root":{"type":"section","data":{"title":"VPN","body":[{"type":"button","data":{"id":"toggle-vpn","label":"Toggle VPN","variant":"accent"}}]}}}
```

Your script receives:

```txt
event {"id":"toggle-vpn","type":"click","source":"popover","button":"left"}
```

Run your command, then print updated `status` and `popover` lines.

## Best Practices

| Practice | Why |
|---|---|
| Print status immediately | The panel should not sit empty while your script warms up. |
| Keep panel labels short | Long labels make the panel jump and crowd other applets. |
| Put detail in the popover | The panel is for glanceable state; popovers are for explanations and controls. |
| Use stable ids | Events are easier to handle when ids do not change between updates. |
| Throttle polling | Most system stats do not need sub-second updates. |
| Send complete updates | Treat each `status` or `popover` line as the current truth. |
| Use variants sparingly | `warning` and `danger` should mean something needs attention. |
| Validate JSON before running | Bad JSON is ignored and logged. |
| Keep stderr quiet | Use stderr for useful diagnostics, not a constant stream. |
| Set only the environment you need | Use `env_clear` and `env` when a child process should not inherit the shell environment. |
| Prefer one script per concern | A small CPU applet is easier to maintain than one giant script for everything. |

## See Also

| Page | Covers |
|---|---|
| [Exec Applet](./exec.md) | Applet config and options. |
| [Components](./exec-components.md) | Popover component fields and component types. |
| [Exec SDK](../applets/exec-sdk.md) | SDK installation and language examples. |
