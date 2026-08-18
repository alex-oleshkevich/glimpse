# Exec components

Exec popovers are component trees. Raw protocol applets send them in `popover` messages, and SDK applets serialize helper objects to the same protocol tree.

Use this page when you need exact component names, fields, defaults, enum values, and emitted events.

## Component node shape

Every component node has a `type` and a `data` object:

```json
{"type":"tile","data":{"id":"refresh","primary":"Refresh"}}
```

The component names below are the names accepted by the current protocol. `section`, `button`, `link_button`, `grid`, `picture`, and `progress` are not exec protocol components.

## Common fields

Most components accept these fields inside `data`:

| Field | Default | Meaning |
|---|---|---|
| `visible` | unset | Hide the component when set to `false`. |
| `tooltip` | unset | Hover text. |
| `css_classes` | `[]` | Extra CSS classes. |
| `styles` | `{}` | Inline style properties accepted by the renderer. Prefer CSS classes for reusable styling. |

## Enums

| Name | Values | Default |
|---|---|---|
| Badge kind | `default`, `success`, `warning`, `error`, `accent` | `default` |
| Status dot status | `neutral`, `success`, `warning`, `error`, `accent` | `neutral` |
| Pager appearance | `dots`, `numbers` | `dots` |
| Popover size | `small`, `medium`, `large`, `wide` | `medium` |
| Mouse button in events | `left`, `middle`, `right`, `other` | none |
| Event source | `status`, `popover` | none |
| Event type | `click`, `toggle`, `change`, `scroll`, `open`, `close` | none |

## Layout components

| Component | Fields | Defaults | Notes |
|---|---|---|---|
| `popover_shell` | `size`, `children`, `footer`, `footer_visible` | `size = "medium"`, `children = []`, `footer = []`, `footer_visible = false` | Full popover layout with optional footer. |
| `row` | `children` | `[]` | Horizontal layout. |
| `column` | `children` | `[]` | Vertical layout. Most applets use this as their root body. |
| `container` | `children` | `[]` | Generic grouped container. |
| `boxed_list` | `children` | `[]` | Native list-style group. |
| `button_row` | `children` | `[]` | Row of compact controls. |
| `scroll` | `child` | required | Scroll wrapper around one child. |
| `circle_box` | `color` | `""` | Circular color swatch or status marker. |
| `separator` | common fields only | none | Divider. |

Example:

```text
popover {"root":{"type":"popover_shell","data":{"size":"medium","children":[{"type":"hero","data":{"title":"System","subtitle":"Live status","icon":"utilities-system-monitor-symbolic"}},{"type":"tile","data":{"id":"refresh","primary":"Refresh","secondary":"Run now","left_icon":"view-refresh-symbolic"}}]}}}
```

## Text and display components

| Component | Fields | Defaults | Notes |
|---|---|---|---|
| `label` | `label`, `xalign`, `wrap` | `xalign` unset, `wrap` unset | Text label. `label` is required. |
| `header` | `label` | none | Section heading. |
| `hero` | `id`, `title`, `subtitle`, `icon`, `icon_size`, `toggle`, `toggle_sensitive`, `separator`, `trailing` | `subtitle = ""`; others unset | Popover header. Emits `toggle` when it has `id` and `toggle` is used. |
| `badge` | `label`, `kind` | `kind = "default"` | Small semantic label. |
| `status_dot` | `status` | `neutral` | Small state dot. |
| `panel_indicator` | `id`, `icon`, `label`, `active`, `checked`, `needs_attention`, `extra` | booleans `false`; others unset | Panel-like indicator inside a popover. Emits `click` when it has `id`. |
| `empty_state` | `title`, `subtitle` | `subtitle` unset | Empty or unavailable state. |
| `spinner` | `spinning` | `true` | Activity indicator. |
| `meter` | `id`, `icon`, `label`, `value`, `min`, `max`, `step`, `text`, `interactive` | `label = ""`, `value = 0`, `min = 0`, `max = 1`, `step = 0.01`, `interactive = false` | Read-only by default. Emits `change` when `interactive = true` and it has `id`. |
| `key_value_grid` | `rows` | `[]` | Key/value facts. Each row has `key` and `value`. |

Example:

```text
popover {"root":{"type":"column","data":{"children":[{"type":"header","data":{"label":"Network"}},{"type":"key_value_grid","data":{"rows":[{"key":"IPv4","value":"10.0.0.42"}]}},{"type":"badge","data":{"label":"Connected","kind":"success"}}]}}}
```

## Tiles and controls

| Component | Fields | Defaults | Events |
|---|---|---|---|
| `tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `right` | optional fields unset | Emits `click` when it has `id`. |
| `segmented_tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `right`, `child`, `expanded` | `expanded = false`; optional fields unset | Emits `toggle` on expand or collapse when it has `id`; emits `click` for its main row when it has `id`. |
| `switch_tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `active` | `active = false`; optional fields unset | Emits `toggle`. `id` is required. |
| `expander_tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `child`, `expanded` | `expanded = false`; optional fields unset | Emits `toggle` when it has `id`. |
| `slider_tile` | `id`, `label`, `left_icon`, `left`, `value`, `min`, `max`, `step`, `page`, `digits`, `snap_step` | `value = 0`, `min = 0`, `max = 1`, `step = 0.01`, `page = 0.1`, `digits = 0`; optional fields unset | Emits debounced `change` values. `id` is required. |
| `choice_tile` | `id`, `primary`, `secondary`, `left_icon`, `left`, `selected` | `selected = false`; optional fields unset | Emits `click` when it has `id`. |
| `choice_list` | `id`, `active`, `choices` | `active` unset, `choices = []` | Emits `change` with the selected choice id. `id` is required. |

`choice_list.choices` entries use `id`, `primary`, optional `secondary`, and optional `icon`.

Example:

```text
popover {"root":{"type":"column","data":{"children":[{"type":"switch_tile","data":{"id":"vpn","primary":"VPN","secondary":"Disconnected","left_icon":"network-vpn-symbolic","active":false}},{"type":"slider_tile","data":{"id":"brightness","label":"Brightness","left_icon":"display-brightness-symbolic","value":0.6,"min":0.0,"max":1.0,"step":0.05}},{"type":"choice_list","data":{"id":"profile","active":"balanced","choices":[{"id":"balanced","primary":"Balanced","secondary":"Recommended"},{"id":"performance","primary":"Performance"}]}}]}}}
```

## Pager components

| Component | Fields | Defaults | Events |
|---|---|---|---|
| `pager_item` | `id`, `label`, `appearance`, `active`, `inactive`, `occupied`, `urgent` | `label = ""`, `appearance = "dots"`, booleans `false` | Emits `click`. `id` is numeric and required. |
| `pager_strip` | `id`, `placeholder`, `items` | `id` unset, `placeholder = false`, `items = []` | Emits `change` when it has `id`. |

Use pager components for workspace or page indicators.

## Calendar components

| Component | Fields | Defaults | Events |
|---|---|---|---|
| `calendar` | `id`, `selected_date`, `event_days` | `id` unset, `event_days = []` | Emits `change` when it has `id`. |
| `date_hero` | `weekday`, `date` | none | Date popover header. |
| `events` | `date`, `events`, `loading` | `events = []`, `loading = false` | Calendar event list. |

`events.events` entries use `id`, `title`, `start`, optional `end`, optional `location`, and `all_day = false`.

## Weather and clock components

| Component | Fields | Defaults | Notes |
|---|---|---|---|
| `weather_forecast_list` | `items` | `[]` | Forecast rows. |
| `weather_hourly_strip` | `items` | `[]` | Hourly forecast strip. |
| `world_clock` | `rows` | `[]` | Timezone rows. |

`weather_forecast_list.items` entries use `day_name`, `icon`, `condition`, `temperatures`, and `is_today = false`.

`weather_hourly_strip.items` entries use `time`, `icon`, and `temperature`.

`world_clock.rows` entries use `name`, `timezone`, `time`, `offset`, and optional `day_label`.

## Battery and privacy components

| Component | Fields | Defaults | Notes |
|---|---|---|---|
| `battery_hero` | `icon`, `percentage`, `fraction`, `state` | none | Battery popover header. |
| `camera_indicator` | `active` | `false` | Camera privacy indicator. |
| `mic_indicator` | `active` | `false` | Microphone privacy indicator. |
| `muted_indicator` | `active` | `false` | Muted audio indicator. |
| `location_indicator` | `active` | `false` | Location privacy indicator. |
| `screencast_indicator` | `active`, `timer_text` | `active = false`, `timer_text` unset | Screencast indicator with optional timer text. |

## Events

Interactive components send `event` lines back to the applet:

| Event | Components | Payload detail |
|---|---|---|
| `click` | `tile`, `choice_tile`, `panel_indicator`, `pager_item`, status items with `id` | Optional `button`. |
| `toggle` | `hero`, `switch_tile`, `expander_tile`, `segmented_tile` | `active` contains the new boolean state. |
| `change` | `meter`, `slider_tile`, `choice_list`, `pager_strip`, `calendar` | `value` contains the new number, string, item id, or date. |
| `scroll` | Status items with `id` | `delta_y` contains the scroll amount. |
| `open`, `close` | Popover lifecycle | `id = "popover"`. |

Raw protocol applets must set `id` on components that should emit events. SDKs can generate private ids for widget-local callbacks; those generated ids are an SDK convenience, not a field you need to write by hand.

## See also

| Page | Use it for |
|---|---|
| [Line Protocol](./exec-protocol.md) | Raw protocol commands, message shapes, and events. |
| [Exec SDK](../applets/exec-sdk.md) | SDK helper classes for these components. |
| [Exec Applet](./exec.md) | Applet package config and runtime behavior. |
