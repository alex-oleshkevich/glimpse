# Exec line protocol

The exec line protocol is the raw interface between the panel and an `exec` applet process. Glimpse starts the child process, writes setup and event lines to stdin, and reads applet output from stdout. Diagnostics belong on stderr.

Use an SDK when possible. Use this page when you are writing a small script, debugging SDK output, or implementing the protocol directly.

## Line format

Each line starts with a command name. Commands with JSON payloads use one space followed by a JSON object:

```text
command {"field":"value"}
```

The raw protocol is line-oriented. Every message must end with a newline. The child process should flush stdout after writing a line.

## Commands

| Direction | Command | Payload | Meaning |
|---|---|---|---|
| Child to Glimpse | `status` | JSON object | Replaces this applet panel status. |
| Child to Glimpse | `popover` | JSON object | Replaces this applet popover tree. |
| Child to Glimpse | `class` | raw text after the space | Sets one applet CSS class suffix. |
| Child to Glimpse | `close-popover` | none | Closes this applet popover. |
| Glimpse to child | `init` | JSON object | Sends instance id and `[exec.options]`. |
| Glimpse to child | `event` | JSON object | Sends user interaction and popover lifecycle events. |

Unknown commands and invalid JSON are ignored and logged. Unknown component types are rejected when Glimpse parses the popover tree.

Example child output:

```text
status {"items":[{"id":"cpu","label":"42%","icon":"cpu-symbolic","tooltip":"CPU usage"}]}
popover {"root":{"type":"popover_shell","data":{"children":[{"type":"hero","data":{"title":"System","subtitle":"CPU 42%"}}]}}}
class sysinfo
close-popover
```

Example input from Glimpse:

```text
init {"instance":"sysinfo","options":{"interval":5,"unit":"celsius"}}
event {"id":"cpu","type":"click","source":"status","button":"left"}
```

## Init messages

`init` is sent by Glimpse after the child process starts.

| Field | Meaning |
|---|---|
| `instance` | Runtime instance id for this applet. |
| `options` | Object from `[exec.options]` in the applet package. |

Use `options` for applet-specific settings instead of hardcoding values in the script.

## Status messages

A `status` message replaces the full list of status items shown in the panel.

```text
status {"items":[
  {"id":"cpu","icon":"cpu-symbolic","label":"12%","tooltip":"CPU","css_classes":["threshold-ok"]},
  {"id":"mem","icon":"memory-symbolic","label":"51%","tooltip":"Memory"}
]}
```

| Field | Default | Meaning |
|---|---|---|
| `id` | unset | Event id. Add it when the item should receive clicks or scrolls. |
| `icon` | unset | Symbolic icon name. |
| `label` | unset | Text shown in the panel. |
| `tooltip` | unset | Hover text. |
| `css_classes` | `[]` | Extra CSS classes for this status item. |

Left-click opens the popover when the applet has popover content. Right-click opens the context menu if one is available. Status items with `id` also receive `click` and `scroll` events.

## Popover messages

A `popover` message replaces the full popover tree. The payload has a `root` field containing a component node, or `null` to clear popover content.

```text
popover {"root":{"type":"popover_shell","data":{"size":"medium","children":[{"type":"hero","data":{"title":"System","subtitle":"Live status","icon":"utilities-system-monitor-symbolic"}},{"type":"tile","data":{"id":"refresh","primary":"Refresh","secondary":"Run now","left_icon":"view-refresh-symbolic"}},{"type":"meter","data":{"label":"Memory","value":0.51,"text":"51%"}}]}}}
```

Clear the popover:

```text
popover {"root":null}
```

Component nodes use this structure:

| Field | Meaning |
|---|---|
| `type` | Component name, such as `popover_shell`, `column`, `tile`, or `meter`. |
| `data` | Component fields. The expected fields depend on `type`. |

Send a complete popover update whenever the content changes. Read [Components](./exec-components.md) for valid component types and fields.

## Class messages

`class` sets one applet-specific class suffix on the panel item and popover. It is not JSON.

```text
class build-status
```

That class is applied as applet-specific styling by the shell. Send it once after startup if the applet needs custom theme selectors.

## Close popover

`close-popover` closes this applet popover. It has no payload.

```text
close-popover
```

Do not send `close_popover {}` or an `action` line from raw protocol applets; those are not accepted child commands by the shell protocol.

## Events

Interactive status items and popover components send `event` lines back to the child process.

| Field | Values | Meaning |
|---|---|---|
| `id` | string | The status item or component id. Popover lifecycle events use `popover`. |
| `type` | `click`, `toggle`, `change`, `scroll`, `open`, `close` | Event kind. |
| `source` | `status`, `popover` | Where the event came from. |
| `button` | `left`, `middle`, `right`, `other` | Mouse button for click events when available. |
| `active` | boolean | Toggle state for toggle events. |
| `value` | JSON value | New value for change events. |
| `delta_y` | number | Scroll delta for scroll events. |

| Source | Event | Payload |
|---|---|---|
| Status item with `id` | `click`, `scroll` | `button` or `delta_y`. |
| `tile`, `choice_tile`, `panel_indicator`, `pager_item` | `click` | `id` identifies the component. |
| `switch_tile`, `expander_tile`, `segmented_tile` | `toggle` | `active = true` or `false`. |
| `slider_tile` | `change` | Numeric `value`. |
| `meter` with `interactive = true` | `change` | Numeric `value`. |
| `choice_list`, `pager_strip`, `calendar` | `change` | Selected id, item id, or date string in `value`. |
| Popover lifecycle | `open`, `close` | `id = "popover"`. |

Components without an `id` are display-only unless the component requires an id.

Example events:

```text
event {"id":"volume","type":"change","source":"popover","value":0.72}
event {"id":"vpn","type":"toggle","source":"popover","active":true}
event {"id":"popover","type":"open","source":"popover"}
```

## Shell starter

```sh
#!/bin/sh

printf "%s\n" "status {\"items\":[{\"id\":\"load\",\"label\":\"starting\",\"icon\":\"utilities-system-monitor-symbolic\"}]}"

while IFS= read -r line; do
  case "$line" in
    init\ *)
      printf "%s\n" "status {\"items\":[{\"id\":\"load\",\"label\":\"ready\",\"icon\":\"utilities-system-monitor-symbolic\"}]}"
      ;;
    event\ *)
      printf "%s\n" "popover {\"root\":{\"type\":\"popover_shell\",\"data\":{\"children\":[{\"type\":\"hero\",\"data\":{\"title\":\"System\",\"subtitle\":\"Last event seen\"}},{\"type\":\"badge\",\"data\":{\"label\":\"seen\",\"kind\":\"success\"}}]}}}"
      ;;
  esac
done
```

This shape is event-driven. For polling, run a background loop and keep reading events in the foreground.

## How to toggle a command

Render a clickable tile:

```text
popover {"root":{"type":"popover_shell","data":{"children":[{"type":"tile","data":{"id":"toggle-vpn","primary":"Toggle VPN","left_icon":"network-vpn-symbolic"}}]}}}
```

Handle the click:

```text
event {"id":"toggle-vpn","type":"click","source":"popover","button":"left"}
```

Run your command, then print updated `status` and `popover` lines.

## Best practices

| Practice | Why |
|---|---|
| Print status immediately | The panel should not sit empty while your script warms up. |
| Write diagnostics to stderr | Stdout is reserved for protocol lines. |
| Send complete updates | Treat each `status` or `popover` line as the current truth. |
| Use stable ids | Events are easier to handle when ids do not change between updates. |
| Keep panel text short | The panel is for glanceable state. |
| Put detail in the popover | Popovers are for explanations and controls. |
| Validate JSON before running | Bad JSON is ignored and logged. |

## See also

| Page | Covers |
|---|---|
| [Exec Applet](./exec.md) | Applet package config and restart behavior. |
| [Components](./exec-components.md) | Popover component fields and component types. |
| [Exec SDK](../applets/exec-sdk.md) | SDK reference by language. |
