# Exec Components

Exec popovers are component trees. Raw protocol applets send them in `popover` messages, and SDK applets build the same tree through typed helpers.

Each node has this shape:

```json
{"type":"section","data":{"title":"System","children":[]}}
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
| `section` | `title = ""`, `subtitle = ""`, `children = []` | A titled group. |
| `card` | `children = []` | A framed group. |
| `row` | `spacing = 0`, `children = []` | Horizontal layout. |
| `column` | `spacing = 0`, `children = []` | Vertical layout. |
| `box` | `spacing = 0`, `children = []` | Explicit horizontal or vertical layout. Requires `orientation`. |
| `grid` | `row_spacing = 0`, `column_spacing = 0`, `children = []` | Two-dimensional layout. |
| `scroll` | no default child | Scrollable content. Requires `child`. |
| `overlay` | `overlays = []`; requires `child` | Stack render-only overlay nodes above a base child. |
| `list_box` | `children = []` | GTK list with one row per child. |
| `expander` | `expanded = false`; requires `label`, `child` | Collapsible disclosure container backed by `gtk::Expander`. |
| `tree_expander` | `hide_expander = false`, `indent_for_depth = false`, `indent_for_icon = false`; requires `child` | GTK tree expander wrapper around one child. |
| `separator` | `orientation = unset` | Visual divider. |

Grid children use:

```json
{"row":0,"column":0,"width":1,"height":1,"child":{"type":"label","data":{"text":"CPU"}}}
```

## Display Components

| Component | Default fields | Use it for |
|---|---|---|
| `hero` | `subtitle = ""`, `icon = unset`; requires `title` | Big header for a popover. |
| `property_list` | `title = ""`, `rows = []` | Key/value facts. |
| `item` | `icon = ""`, `sublabel = ""`, `right = unset`; requires `label` | Display-only row with an optional right-side component. |
| `action_item` | `icon = ""`, `sublabel = ""`, `right = unset`, `enabled = true`; requires `id`, `label` | Clickable row with an optional render-only right-side component. |
| `empty_state` | `subtitle = ""`; requires `title` | Friendly empty message. |
| `badge` | requires `label` | Small pill label. |
| `status` | common fields only | Small status marker. |
| `meter` | `icon = unset`, `label = ""`, `min = 0`, `max = 1`, `step = 0.01`, `text = unset`, `interactive = false`; requires `value` | Progress row or slider row. |
| `progress` | `max = 1`, `show_text = false`, `text = unset`; requires `value` | Progress bar. |
| `level_bar` | `min = 0`, `max = 1`, `mode = continuous`; requires `value` | Native GTK level indicator. Modes: `continuous`, `discrete`. |
| `copyable` | `label = ""`; requires `value` | Text row with copy action. |
| `spinner` | `spinning = true` | Loading indicator. |
| `label` | `wrap = false`, `xalign = unset`, `selectable = false`; requires `text` | Text. |
| `icon` | `pixel_size = unset`; requires `icon` | Symbolic icon. |
| `image` | `pixel_size = unset`; requires `icon` | Image from icon name or path. |
| `picture` | `content_fit = contain`; requires `path` | Image file rendered with `gtk::Picture`. Fits: `fill`, `contain`, `cover`, `scale_down`. |
| `button` | `label = unset`, `icon = unset`, `enabled = true`, `variant = flat`; requires `id` for events | Button. `icon` is an icon-name string. Variants: `primary`, `secondary`, `compact`, `flat`, `danger`. |
| `link_button` | `label = unset`; requires `uri` | Link-style button that opens a URI through GTK. Does not emit applet events. |
| `menu_button` | `label = unset`, `icon = unset`; requires `popover` | Button that opens a rendered popover child. The button itself does not emit applet events. |
| `switch` | `label = unset`, `active = false`; requires `id` | Toggle switch. |
| `toggle_button` | `label = unset`, `active = false`; requires `id` | Button-like toggle that emits `toggle` events. |
| `checkbox` | `label = unset`, `active = false`; requires `id` | Checkbox. |
| `slider` | `orientation = unset`, `draw_value = false`; requires `id`, `min`, `max`, `step`, `value` | Slider. |
| `select` | `items = []`, `selected = unset`; requires `id` | Select control. |

## Button

Use `button` for explicit commands. Buttons do not accept child widgets or menu actions.

```txt
popover {"root":{"type":"button","data":{"id":"refresh","label":"Refresh","icon":"view-refresh-symbolic","variant":"primary"}}}
```

## Link Button

Use `link_button` for external documentation or support links. It opens the URI directly through GTK and does not send an applet event.

```txt
popover {"root":{"type":"link_button","data":{"uri":"https://example.com/docs","label":"Docs"}}}
```

## Expander

Use `expander` for optional detail content that should stay in the same popover.

```txt
popover {"root":{"type":"expander","data":{"label":"Details","expanded":true,"child":{"type":"label","data":{"text":"More"}}}}}
```

## Tree Expander

Use `tree_expander` for a GTK tree expander wrapper around a child node.

```txt
popover {"root":{"type":"tree_expander","data":{"child":{"type":"label","data":{"text":"Nested"}},"hide_expander":true,"indent_for_depth":true,"indent_for_icon":true}}}
```

## Menu Button

Use `menu_button` for compact controls that open rendered popover content. Controls inside the nested `popover` node still emit their own events.

```txt
popover {"root":{"type":"menu_button","data":{"label":"More","icon":"open-menu-symbolic","popover":{"type":"label","data":{"text":"Menu content"}}}}}
```

## Overlay

Use `overlay` to render badges or other status affordances above a base component.

```txt
popover {"root":{"type":"overlay","data":{"child":{"type":"picture","data":{"path":"/home/me/.cache/avatar.png","content_fit":"cover"}},"overlays":[{"type":"badge","data":{"label":"Live","halign":"end","valign":"start"}}]}}}
```

## List Box

Use `list_box` for native GTK list rows. Each child is rendered inside its own row.

```txt
popover {"root":{"type":"list_box","data":{"children":[{"type":"label","data":{"text":"First"}},{"type":"badge","data":{"label":"Second"}}]}}}
```

## Level Bar

Use `level_bar` for native GTK level indicators.

```txt
popover {"root":{"type":"level_bar","data":{"value":0.7,"min":0,"max":1,"mode":"continuous"}}}
```

## See Also

| Page | Covers |
|---|---|
| [Exec Applet](./exec.md) | Applet config and options. |
| [Line Protocol](./exec-protocol.md) | Raw protocol commands, message shapes, and events. |
| [Exec SDK](../applets/exec-sdk.md) | SDK installation and language examples. |
