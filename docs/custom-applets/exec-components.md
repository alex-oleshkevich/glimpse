# Exec Components

Exec popovers are component trees. Raw protocol applets send them in `popover` messages, and SDK applets serialize their helper objects to the same tree.

Every node has this shape:

```json
{"type":"label","data":{"label":"Ready"}}
```

The component names below are the names accepted by the current protocol. Names such as `section`, `button`, `link_button`, `grid`, `picture`, and `progress` are not exec protocol components.

## Common Fields

Most components accept these fields through `data`:

| Field | Default | Meaning |
| --- | --- | --- |
| `visible` | visible | Hide the component when set to `false`. |
| `tooltip` | unset | Hover text. |
| `css_classes` | `[]` | Extra CSS classes for theme authors. |
| `styles` | `{}` | Inline style values for supported renderer paths. Prefer CSS classes for normal theming. |

## Layout Components

| Component | Key fields | Use it for |
| --- | --- | --- |
| `popover_shell` | `size`, `children`, `footer`, `footer_visible` | Full popover layout with optional footer. Sizes: `small`, `medium`, `large`, `wide`. |
| `row` | `children` | Horizontal grouping. |
| `column` | `children` | Vertical grouping. |
| `container` | `children` | Generic grouped content. |
| `boxed_list` | `children` | List-style grouped rows. |
| `button_row` | `children` | Row of compact controls. |
| `scroll` | `child` | Scrollable child content. |
| `separator` | common fields only | Divider. |
| `circle_box` | `color` | Small color dot or swatch. |

Example:

```txt
popover {"root":{"type":"popover_shell","data":{"size":"medium","children":[{"type":"hero","data":{"title":"System","subtitle":"Live status","icon":"utilities-system-monitor-symbolic"}},{"type":"tile","data":{"id":"refresh","primary":"Refresh","secondary":"Run now","left_icon":"view-refresh-symbolic"}}]}}}
```

## Display Components

| Component | Key fields | Use it for |
| --- | --- | --- |
| `label` | `label`, `xalign`, `wrap` | Plain text. |
| `header` | `label` | Section heading. |
| `hero` | `title`, `subtitle`, `icon`, `icon_size`, `toggle`, `toggle_sensitive`, `separator`, `trailing` | Popover header. |
| `badge` | `label`, `kind` | Small status label. Kinds: `default`, `success`, `warning`, `error`, `accent`. |
| `status_dot` | `status` | Small state dot. Statuses: `neutral`, `success`, `warning`, `error`, `accent`. |
| `panel_indicator` | `id`, `icon`, `label`, `active`, `checked`, `needs_attention`, `extra` | Panel-like indicator inside a popover. |
| `empty_state` | `title`, `subtitle` | Empty or unavailable state. |
| `spinner` | `spinning` | Loading indicator. |
| `meter` | `id`, `icon`, `label`, `value`, `min`, `max`, `step`, `text` | Read-only meter. |
| `key_value_grid` | `rows` | Key/value facts. Each row has `key` and `value`. |

Example:

```txt
popover {"root":{"type":"column","data":{"children":[{"type":"header","data":{"label":"Network"}},{"type":"key_value_grid","data":{"rows":[{"key":"IPv4","value":"10.0.0.42"}]}},{"type":"badge","data":{"label":"Connected","kind":"success"}}]}}}
```

## Tile And Control Components

| Component | Key fields | Event behavior |
| --- | --- | --- |
| `tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `right` | Emits `click` when it has `id`. |
| `segmented_tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `right`, `child`, `expanded` | Emits `toggle` on expand/collapse; emits `click` when `id` is set. |
| `switch_tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `active` | Emits `toggle`. |
| `expander_tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `child`, `expanded` | Emits `toggle` when `id` is set. |
| `slider_tile` | `id`, `label`, `left_icon`, `left`, `value`, `min`, `max`, `step`, `page`, `digits`, `snap_step` | Emits debounced `change` values. |
| `choice_tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `selected` | Emits `click` when `id` is set. |
| `choice_list` | `id`, `active`, `choices` | Emits `change` with the selected choice id. |

Example:

```txt
popover {"root":{"type":"column","data":{"children":[{"type":"switch_tile","data":{"id":"vpn","primary":"VPN","secondary":"Disconnected","left_icon":"network-vpn-symbolic","active":false}},{"type":"slider_tile","data":{"id":"brightness","label":"Brightness","left_icon":"display-brightness-symbolic","value":0.6,"min":0.0,"max":1.0,"step":0.05}},{"type":"choice_list","data":{"id":"profile","active":"balanced","choices":[{"id":"balanced","primary":"Balanced","secondary":"Recommended"},{"id":"performance","primary":"Performance"}]}}]}}}
```

## Specialized Components

These components mirror shell UI patterns and are useful when a custom applet wants to feel native:

| Component | Use it for |
| --- | --- |
| `pager_item`, `pager_strip` | Workspace or page indicators. |
| `camera_indicator`, `mic_indicator`, `muted_indicator`, `screencast_indicator`, `location_indicator` | Privacy/status indicators. |
| `calendar` | Calendar view with selected date and marked event days. |
| `battery_hero` | Battery popover header. |
| `date_hero` | Date popover header. |
| `events` | Calendar event list. |
| `weather_forecast_list`, `weather_hourly_strip` | Weather forecast rows. |
| `world_clock` | Timezone rows. |

## Event Payloads

Interactive components send events back to the applet:

| Event | Typical components | Payload detail |
| --- | --- | --- |
| `click` | `tile`, `choice_tile`, `panel_indicator`, `pager_item`, status items with `id` | Optional `button` for status clicks. |
| `toggle` | `switch_tile`, `expander_tile`, `segmented_tile` | `active = true` or `false`. |
| `change` | `slider_tile`, `choice_list`, `pager_strip`, `calendar` | `value` contains the new number, string, item id, or date. |
| `open`, `close` | Popover lifecycle | `id = "popover"`. |

Status item scroll events are documented in [Line Protocol](./exec-protocol.md).

## See Also

| Page | Covers |
| --- | --- |
| [Exec Applet](./exec.md) | Applet config and options. |
| [Line Protocol](./exec-protocol.md) | Raw protocol commands, message shapes, and events. |
| [Exec SDK](../applets/exec-sdk.md) | SDK installation and language examples. |
