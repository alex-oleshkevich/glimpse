# Exec Components

Exec popovers are component trees. Raw protocol applets send them in `popover` messages, and SDK applets build the same tree through typed helpers.

Each node has this shape:

```json
{"type":"section","data":{"title":"System","body":[]}}
```

| Field | Meaning |
|---|---|
| `type` | Component name. |
| `data` | Component fields. The expected fields depend on `type`. |

## Common Component Fields

Most popover components accept these fields:

| Field | Default | Values | Meaning |
|---|---|---|---|
| `id` | unset | string | Required for interactive components. Used in events. |
| `visible` | unset, treated as visible | boolean | Hide or show the component. |
| `hexpand` / `vexpand` | unset, treated as `false` | boolean | Let the component take extra space. |
| `halign` / `valign` | unset | `fill`, `start`, `end`, `center`, `baseline` | Alignment. |
| `tooltip` | unset | string | Hover text. |
| `variant` | unset, treated as `normal` | `normal`, `muted`, `accent`, `success`, `warning`, `danger` | Visual emphasis. |

## Layout Components

| Component | Default fields | Use it for |
|---|---|---|
| `section` | `header = {title, subtitle?}` (required), `body = []` | A titled group. |
| `collapsible` | `header = {title, subtitle?}` (required), `expanded = false`, `body = []` | Expandable group. |
| `card` | `children = []` | A framed group. |
| `row` | `spacing = 0`, `children = []` | Horizontal layout. |
| `column` | `spacing = 0`, `children = []` | Vertical layout. |
| `box` | `spacing = 0`, `children = []` | Explicit horizontal or vertical layout. Requires `orientation`. |
| `grid` | `row_spacing = 0`, `column_spacing = 0`, `children = []` | Two-dimensional layout. |
| `scroll` | no default child | Scrollable content. Requires `child`. |
| `separator` | `orientation = unset` | Visual divider. |

Grid children use:

```json
{"row":0,"column":0,"width":1,"height":1,"child":{"type":"label","data":{"text":"CPU"}}}
```

## Display Components

| Component | Default fields | Use it for |
|---|---|---|
| `hero` | `subtitle = ""`, `icon = unset`; requires `title` | Big header for a popover. |
| `item` | `left = unset`, `label = ""`, `right = unset`, `clickable = false`, `menu = []` | Standard list row. |
| `collapsible_item` | `left = unset`, `label = ""`, `right = unset`, `expanded = false`, `body = []` | Expandable list row. |
| `action_row` | `subtitle = ""`, `meta = ""`, `icon = unset`; requires `title` | Clickable-looking row with summary text. |
| `action_menu` | `header = unset`, `items = []` | Menu of script-defined actions. |
| `detail_grid` | `rows = []` | Key/value facts. |
| `empty_state` | `subtitle = ""`; requires `title` | Friendly empty message. |
| `badge` | requires `label` | Small pill label. |
| `status` | common fields only | Small status marker. |
| `meter` | `icon = unset`, `label = ""`, `min = 0`, `max = 1`, `step = 0.01`, `text = unset`, `interactive = false`; requires `value` | Progress row or slider row. |
| `progress` | `max = 1`, `show_text = false`, `text = unset`; requires `value` | Progress bar. |
| `copyable` | `label = ""`; requires `value` | Text row with copy action. |
| `toast` | `icon = unset`, `message = ""`, `action = unset`; requires `title` | Inline notice. |
| `spinner` | `spinning = true` | Loading indicator. |
| `label` | `wrap = false`, `xalign = unset`, `selectable = false`; requires `text` | Text. |
| `icon` | `pixel_size = unset`; requires `icon` | Symbolic icon. |
| `image` | `pixel_size = unset`; requires `icon` | Image from icon name or path. |
| `button` | `label = unset`, `icon = unset`, `child = unset`; requires `id` for events | Button. |
| `switch` | `label = unset`, `active = false`; requires `id` | Toggle switch. |
| `checkbox` | `label = unset`, `active = false`; requires `id` | Checkbox. |
| `scale` | `orientation = unset`, `draw_value = false`; requires `id`, `min`, `max`, `step`, `value` | Slider. |
| `dropdown` | `items = []`, `selected = unset`; requires `id` | Dropdown. |

## Action Menu

Use `action_menu` when a compact list of actions is better than separate buttons.

```txt
popover {"root":{"type":"action_menu","data":{
  "header":"Power profile",
  "items":[
    {"id":"power-saver","label":"Power Saver","checked":false},
    {"id":"balanced","label":"Balanced","checked":true},
    {"id":"performance","label":"Performance","checked":false}
  ]
}}}
```

## See Also

| Page | Covers |
|---|---|
| [Exec Applet](./exec.md) | Applet config and options. |
| [Line Protocol](./exec-protocol.md) | Raw protocol commands, message shapes, and events. |
| [Exec SDK](../applets/exec-sdk.md) | SDK installation and language examples. |
