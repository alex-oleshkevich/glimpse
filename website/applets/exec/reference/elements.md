# Elements

Every element an applet can draw, what each holds, and its props. The SDKs name them the same way:
`actionrow` is `ActionRow` in TypeScript and Python, `sdk.ActionRow` in Go and `action_row()` in
Rust. Props are camelCase here and in TypeScript, snake_case in Python and Rust (`class_name`,
`on_toggle`), and Go struct fields (`ClassName`, `OnToggle`).

## Parent table

The root holds `indicator` and `popover` elements. The panel shows the first `popover`.

| Parent | Children |
| --- | --- |
| root | `indicator`, `popover` |
| `popover` | at most one `addon`, and any body element |
| `addon` | at most one of `readout`, `switch`, `container`, `swatch` |
| `container` | at most one body element except `section` |
| `section` | body elements except `section` |
| `revealer` | any number of body elements except `section` |
| `stack` | body elements except `section` |
| `row` | at most one of `image`, `readout`, `spinner`, drawn at its end |
| `actionrow` | at most one of `image`, `readout`, `spinner`, drawn at its end, and at most one `start` |
| `meterrow` | at most one of `image`, `readout`, `spinner`, drawn at its end |
| `expanderrow` | at most one `end` and one `start`; drawer children: `actionrow`, `togglerow`, `meterrow`, `sliderrow`, `expanderrow`, `factlist`, `choicelist`, `row` |
| `end` | at most one of `image`, `readout`, `spinner` |
| `start` | at most one of `image`, `button`, `readout`, `spinner`, `dots`, `swatch` |
| `sliderrow` | at most one `start` |
| `togglerow` | nothing |
| `box` | body elements |
| anything else | nothing |

Body elements are `section`, `row`, `actionrow`, `togglerow`,
`meterrow`, `expanderrow`, `sliderrow`, `entry`, `placeholder`, `box`,
`container`,
`label`, `image`, `button`, `switch`, `scale`, `spinner`, `separator`,
`factlist`, `choicelist`, `readout`, `notice`, `calendar`, `dots`, `dropdown`,
`togglebutton`, `checkbutton`, `revealer`, `stack`, `rangebar`, `bars`,
`sparkline` and `unsupported`.

An array prop with more than 64 items, at any depth, rejects the commit. The `values` and
`tooltips` props of `bars` and `sparkline` may hold 366.

An unknown `type` is `unsupported`. Its name is the type string after text cleaning,
capped at 64 characters. Markup in that name is kept literal.

A child on a leaf is rejected, as are a `section` inside a `section` and any
pair the table does not list.

## Elements

`type` is one of the lowercase names below. Props use camelCase. Every element takes
`className`, one class from this list, or the key is dropped. `addon`, `container`, `row`,
`actionrow`, `togglerow`, `meterrow`, `expanderrow`, `end`, `start`, `sliderrow` and
`swatch` do not take `className`:

`dim-label`, `caption`, `heading`, `title-1`, `title-2`, `title-3`, `title-4`,
`numeric`, `accent`, `success`, `warning`, `error`, `flat`, `pill`, `circular`,
`column`, `network-popover__rule`,
`bluetooth-popover`, `network-popover`.

There is no applet CSS.

Unless a row below says otherwise:

- Strings that are labels, titles, tooltips, subtitles, descriptions, counts or
  placeholders go through text cleaning and a cap of 256 characters. Cleaning drops
  control characters and bidi overrides, folds whitespace, trims, and appends `…` when
  the cap cuts the string. An empty result removes the prop.
- `icon` and `overlay` must match `^[A-Za-z0-9_.-]+$`. Anything else is dropped.
- Strings that are row `value`s are cleaned and capped the same way.
- An enum prop (`tone`, `typography`, `control`, `severity`) with any string its table
  does not list is dropped, as if absent.
- Booleans default to `false`, except `sensitive` and `bars.highlightLast`, which default to `true`.
- A missing object defaults each prop as the tables say.
- Inputs are controlled: pass the value, and update it from the callback, or the next render puts
  the old value back.

### `indicator`

The chip on the bar. Only under the root, so put indicators at the root of your tree. No children.
A left click opens the popover and never reaches `onPress`; `onPress` gets `2` for a middle click,
`3` for a right click, and the raw code for any other button. `severity` `warning` and `error` color
the chip; `info` looks the same as none.

| Prop | Type | Notes |
| --- | --- | --- |
| `icon` | string | Icon name |
| `text` | string | Chip label |
| `tooltip` | string | |
| `badge` | string | Cleaned and capped at 8 characters |
| `overlay` | string | Icon name, drawn over the first icon |
| `dot` | string | Dot color in CSS, for example `#e01b24`. A value that is not a color shows no dot |
| `severity` | `info`, `warning`, `error` | |
| `attention` | boolean | Shows a dot while no badge is visible |
| `notice` | boolean | A rounded accent background behind icon and label |
| `className` | class | |
| `onPress` | boolean | |
| `onScroll` | boolean | |

### `popover`

The card that opens on a left click. Under the root. `icon`, `title` and `subtitle` render the shell's header. The popover body
scrolls past 60% of the monitor height.

| Prop | Type | Notes |
| --- | --- | --- |
| `className` | class | |
| `icon` | string | Themed icon in the shell header |
| `title` | string | Header title. The header is shown when this is set |
| `subtitle` | string | Header subtitle |
| `size` | `regular`, `wide` | Width of the card. Defaults to regular |

Children follow the parent table. The order of `children` is the order on screen.

### `addon`

One control in the popover header's end slot. At most one, under `popover`. Its one child is
`readout`, `switch`, `container` or `swatch`. It does not take `className`.

### `container`

A sized, painted box around one child. Use it for a colored block or to fix a width. A body
element. At most one child, any body element except `section`. It does not take
`className`.

| Prop | Type | Notes |
| --- | --- | --- |
| `width` | number | Clamped to -1..=1024. -1 is the natural width |
| `height` | number | Clamped to -1..=1024. -1 is the natural height |
| `background` | paint | `red`, `orange`, `yellow`, `green`, `blue`, `purple`, `light`, `dark` |
| `color` | paint | The same paint names, applied to the text |
| `align` | align | `fill`, `start`, `end`, `center`, `baseline`. Applied to the child |

### `section`

A titled group of body elements. A body element. It holds body elements except another
`section`. `count` sits beside the title.

| Prop | Type |
| --- | --- |
| `title` | string |
| `count` | string |
| `className` | class |

### `row`

Rows are the main building block of a popover. `row`, `actionrow`, `togglerow`, `meterrow` and
`expanderrow` share the props in this table; `sliderrow` does not. A body element. A static row: it
fires no events. At most one end child: `image`, `readout`
or `spinner`.

| Prop | Type | Notes |
| --- | --- | --- |
| `icon` | string | Themed icon name |
| `title` | string | |
| `subtitle` | string | |
| `value` | string | A short value at the end, such as `5 GHz` |
| `tone` | `warning`, `error`, `destructive` | Absent is normal |
| `inactive` | boolean | `false`. Fades the row |
| `tooltip` | string | Plain text |
| `typography` | `numeric`, `monospace` | Absent is default. `numeric` suits figures that change |

Inactive action, toggle and meter rows ignore input, are insensitive, leave Tab navigation and fade
once. Use `subtitle` to say why a row is inactive. Updates from your applet still render.

### `actionrow`

A row you click. A body element. At most one end child: `image`, `readout` or `spinner`, and at most one
`start`.
`onActivate` sends `[]` when the row is activated; inactive rows emit nothing.

| Prop | Type | Notes |
| --- | --- | --- |
| `icon` | string | Themed icon name |
| `title` | string | |
| `subtitle` | string | |
| `value` | string | |
| `tone` | `warning`, `error`, `destructive` | Absent is normal |
| `inactive` | boolean | `false`. Suppresses primary user input |
| `tooltip` | string | Plain text |
| `typography` | `numeric`, `monospace` | Absent is default |
| `busy` | boolean | |
| `onActivate` | boolean | |

### `togglerow`

A row with a switch or a check box. It is controlled: pass `active` and update it in `onToggle`.
A leaf. `control` selects a check or switch control. `onToggle` sends
`[active]` with a `seq`; inactive rows emit nothing. The actual `active` value remains visible.

| Prop | Type | Notes |
| --- | --- | --- |
| `icon` | string | Themed icon name |
| `title` | string | |
| `subtitle` | string | |
| `value` | string | |
| `tone` | `warning`, `error`, `destructive` | Absent is normal |
| `inactive` | boolean | `false`. Suppresses primary user input |
| `tooltip` | string | Plain text |
| `typography` | `numeric`, `monospace` | Absent is default |
| `busy` | boolean | |
| `control` | `check`, `switch` | Absent is `switch` |
| `active` | boolean | Controlled by `onToggle` |
| `onToggle` | boolean | |

### `meterrow`

A row with a level bar, such as disk use. A body element. At most one end child: `image`, `readout` or `spinner`.
Its level is a fraction from `0` to `1`. No events.

| Prop | Type | Notes |
| --- | --- | --- |
| `icon` | string | Themed icon name |
| `title` | string | |
| `subtitle` | string | |
| `value` | string | |
| `tone` | `warning`, `error`, `destructive` | Absent is normal |
| `inactive` | boolean | `false`. Suppresses primary user input |
| `tooltip` | string | Plain text |
| `typography` | `numeric`, `monospace` | Absent is default |
| `busy` | boolean | |
| `level` | number | Clamped to `0` to `1`. Absent or non-finite draws no bar |

### `expanderrow`

A row that opens a drawer of rows below it. A body element. Its head may contain one `end` element with at most one
`image`, `readout` or `spinner` child, and one `start` element. Its other children fill the drawer as
listed in the parent table. `onToggle` sends `[open]` with a `seq` in every state.
Expansion is inspection, so the head stays activatable when inactive. Every drawer participates
in the accordion, so the panel closes other drawers when one opens. `inactive` never propagates to drawer children.

| Prop | Type | Notes |
| --- | --- | --- |
| `icon` | string | Themed icon name |
| `title` | string | |
| `subtitle` | string | |
| `value` | string | |
| `tone` | `warning`, `error`, `destructive` | Absent is normal |
| `inactive` | boolean | `false`. Fades the head; expansion remains available |
| `tooltip` | string | Plain text |
| `typography` | `numeric`, `monospace` | Absent is default |
| `busy` | boolean | |
| `open` | boolean | Controlled by `onToggle` |
| `tint` | boolean | `false`. Tone colors the whole head and drawer card |
| `titleLines` | number | `1`. Non-finite, non-integer or less than 1 becomes 1; greater than 4 becomes 4 |
| `onToggle` | boolean | |

### `end`

A slot at the end of an `expanderrow` head. Only under `expanderrow`. It takes no props and holds at most one `image`,
`readout` or `spinner`, drawn in the head's end slot. No events.

### `start`

A slot for the leading control of a row. Under `sliderrow`, `actionrow` or `expanderrow`. It takes no props and holds at most one
`image`, `button`, `readout`, `spinner`, `dots` or `swatch`. Under `sliderrow` it is drawn left of the
scale; under a row it takes the icon's place, centered, and hides `icon`. Empty slots are hidden.
No events.

### `sliderrow`

A row with a slider. It does not take the shared row props. Holds at most one `start`. `onMove`
sends `[value]` during a drag with `seq: null`; `onChange` sends `[value]` once at drag end or on a
keyboard step, with a `seq`, so update `value` there.
Bounds: `min` `0`, `max` `1`, `step` `0.01`; a `step` that is not greater than `0`
becomes `0.01`; `min >= max` resets both to `0` and `1`. A non-finite number is dropped and
takes its default. `value` is clamped to `min` to `max`; a missing `value` is `min`. A `set`
of `value` that arrives during a drag is dropped; the release wins, and a later `set` applies normally.
Turning `inactive` on during a drag cancels it without a final `onChange`. Inactive rows
suppress `onMove` and `onChange`. Only the scale is insensitive and faded;
the start slot's owner controls its sensitivity. Backend `set` updates still render.

| Prop | Type | Notes |
| --- | --- | --- |
| `title` | string | Accessible name of the value control |
| `subtitle` | string | Plain text, cleaned and capped at 256 characters |
| `value` | number | `min`. Controlled by `onChange` |
| `min` | number | `0` |
| `max` | number | `1` |
| `step` | number | `0.01`. Keyboard step |
| `tone` | `warning`, `error`, `destructive` | Absent is normal |
| `inactive` | boolean | `false`. Disables scale input |
| `tooltip` | string | Plain text |
| `onMove` | boolean | |
| `onChange` | boolean | |

### `entry`

A text field. A leaf. `value` is not text-cleaned. Control characters and bidi overrides are stripped
in place, the rest is kept, including leading and trailing spaces, and the result is
capped at 4096 characters with no ellipsis.

| Prop | Type | Default |
| --- | --- | --- |
| `placeholder` | string | absent. This one is text-cleaned |
| `value` | string | `""` |
| `className` | class | |
| `onChange` | boolean | |
| `onSubmit` | boolean | |

### `placeholder`

An empty state: an icon, a title and a line of explanation. A leaf.

| Prop | Type |
| --- | --- |
| `icon` | string |
| `title` | string |
| `description` | string |
| `className` | class |

### `box`

Lays out body elements in a row or a column. A body element. Holds body elements.

| Prop | Type | Default |
| --- | --- | --- |
| `orientation` | `horizontal`, `vertical` | `horizontal` |
| `spacing` | number | `0`, clamped to `0` to `24` |
| `homogeneous` | boolean | `false` |
| `halign`, `valign` | `fill`, `start`, `end`, `center`, `baseline` | absent, so the widget's own align is left alone |
| `hexpand`, `vexpand` | boolean | `false` |
| `className` | class | |

### `label`

Plain text. A leaf. The words are the `text` prop, plain text, never markup.

| Prop | Type | Default |
| --- | --- | --- |
| `text` | string | absent |
| `wrap` | boolean | `false` |
| `xalign` | number | absent. When set, clamped to `0` to `1` |
| `ellipsize` | `none`, `start`, `middle`, `end` | absent |
| `lines` | number | `0`, meaning no extra line limit. Rounded and clamped to `0` to `8` |
| `className` | class | |

### `image`

A themed icon. A leaf. `icon` is a themed name, never a path.
When set, `tooltip` also provides the accessible name.

| Prop | Type | Default |
| --- | --- | --- |
| `icon` | string | absent |
| `tooltip` | string | absent |
| `pixelSize` | number | absent. When set, rounded and clamped to `8` to `64` |
| `className` | class | |

### `swatch`

A leaf: a chip filled with `color`. Only in `addon` and `start`.

| Prop | Type | Default |
| --- | --- | --- |
| `color` | string | absent, an empty chip. A CSS color: `#e0563f`, `rgb(224 86 63)`, a name; capped at 256 characters; one that does not parse draws nothing |
| `tooltip` | string | absent |
| `size` | `regular`, `large` | `regular`. `large` is the addon's hero size |

### `button`

A push button with `text`, `icon` or both. A leaf. The label is the `text` prop.
Give an icon-only button a `tooltip`; it also provides the accessible name.

| Prop | Type | Default |
| --- | --- | --- |
| `text` | string | absent |
| `icon` | string | absent |
| `tooltip` | string | absent |
| `sensitive` | boolean | `true` |
| `className` | class | |
| `onClick` | boolean | |

### `switch`

An on/off switch. Most often it sits in an `addon`; in a list, use a `togglerow`. A leaf.

| Prop | Type | Default |
| --- | --- | --- |
| `active` | boolean | `false` |
| `sensitive` | boolean | `true` |
| `className` | class | |
| `onToggle` | boolean | |

### `scale`

A bare slider. Prefer a `sliderrow` in a list. A leaf. `min` must be less than `max`. When a set breaks that, both return to `0` and
`1` and `value` is clamped into the range. `value` is also clamped when the range is
already valid.

| Prop | Type | Default |
| --- | --- | --- |
| `value` | number | `0` |
| `min` | number | `0` |
| `max` | number | `1` |
| `step` | number | `0`. A negative step becomes `0` |
| `sensitive` | boolean | `true` |
| `className` | class | |
| `onChange` | boolean | |

### `spinner`

A busy spinner. A leaf. No props besides `className`.

### `factlist`

Label and value pairs. A leaf. No events.

| Prop | Type | Notes |
| --- | --- | --- |
| `facts` | array of `{label, value, warning?}` | At most 64 items |

### `choicelist`

A visible list of up to 64 choices, one selected, each with an optional detail and icon. A leaf.
`selected` and `onSelect` use the index. `onSelect` sends `[index]`, a number, with a `seq`.

| Prop | Type | Notes |
| --- | --- | --- |
| `choices` | array of `{label, detail?, icon?, warning?}` | At most 64 items |
| `selected` | number | |
| `className` | class | |
| `onSelect` | boolean | |

### `readout`

A large value with a unit, for an `addon`, a row's end or a `start` slot. A leaf.

| Prop | Type |
| --- | --- |
| `value` | string |
| `unit` | string |
| `className` | class |

### `notice`

A highlighted message with a severity. A leaf. `onActivate` makes it a button.

| Prop | Type |
| --- | --- |
| `icon` | string |
| `title` | string |
| `subtitle` | string |
| `severity` | `info`, `warning` or `error` |
| `className` | class |
| `onActivate` | boolean |

### `calendar`

A month grid. A leaf. `today` and `selected` are `YYYY-MM-DD`; a date that is not a real day is dropped.
`onMonth` sends `[year, month]` when the user pages to another month, and `onSelect`
sends `["YYYY-MM-DD"]`. Neither fires for a state set by the applet. Each event marks a day with up to three tone dots.

| Prop | Type | Notes |
| --- | --- | --- |
| `year` | number | Clamped to `1` to `9999`. With `month`, the month shown |
| `month` | number | Clamped to `1` to `12` |
| `selected` | date | Absent clears the selection |
| `today` | date | |
| `events` | array of `{date, colors}` | At most 64 items. `colors` holds at most 64 tones from `accent`, `dim`, `success`, `warning`, `error`; the widget draws the first three |
| `className` | class | |
| `onMonth` | boolean | |
| `onSelect` | boolean | |

### `dots`

A row of dots for a status at a glance. A leaf. One tone in `colors` draws one dot, in order.
For example, `[success, error]` draws a success dot, then an error dot.

| Prop | Type | Notes |
| --- | --- | --- |
| `colors` | array of tone | Up to 64 tones: `accent`, `dim`, `success`, `warning`, `error`. A tone is a named theme color, so the dots change with the theme |
| `uniform` | boolean | Draw every dot in the faded text color instead of `colors` |
| `size` | number | Dot diameter in pixels, 2 to 16. Defaults to 4 |
| `max` | number | How many dots to show, 1 to 3. Defaults to 3. Extra `colors` are ignored, so at most 3 dots ever show |
| `className` | class | |

### `dropdown`

A compact choice from up to 64 strings. A leaf. `selected` is the index. `onSelect` sends `[index]`, a number, with a `seq`.

| Prop | Type | Notes |
| --- | --- | --- |
| `options` | array of string | At most 64 items |
| `selected` | number | |
| `className` | class | |
| `onSelect` | boolean | |

### `togglebutton`

A button that stays pressed. A leaf. `onToggle` sends `[active]` with a `seq`.

| Prop | Type |
| --- | --- |
| `label` | string |
| `icon` | string |
| `active` | boolean |
| `className` | class |
| `onToggle` | boolean |

### `checkbutton`

A check box with a label. A leaf. `onToggle` sends `[active]` with a `seq`.

| Prop | Type |
| --- | --- |
| `label` | string |
| `active` | boolean |
| `className` | class |
| `onToggle` | boolean |

### `revealer`

Shows or hides its children with a transition. Holds any number of body children, including none.

| Prop | Type | Default |
| --- | --- | --- |
| `revealed` | boolean | `false` |
| `transition` | `slide-down`, `crossfade` or `none` | `slide-down` |
| `className` | class | |

### `stack`

Holds any number of body children. Children are addressed by index, in order, and the
child at `visible` is shown.

| Prop | Type | Default |
| --- | --- | --- |
| `visible` | number | `0` |
| `className` | class | |

### `rangebar`

A range from `low` to `high` on a `min` to `max` scale, with an optional `now` marker, such as a
day's temperatures. A leaf. Non-finite numbers are dropped; `min` must be below
`max` or the scale is `0` to `1`; `low`, `high` and `now` are clamped into it and `high`
is raised to `low`.

| Prop | Type | Default |
| --- | --- | --- |
| `min` | number | `0` |
| `max` | number | `1` |
| `low` | number | `0` |
| `high` | number | `0` |
| `now` | number | absent |
| `className` | class | |

### `bars`

A small bar chart of up to 366 values. A leaf. One bar per value, scaled to the largest value or to `maximum`. A `null`, negative or
non-finite value is no data and draws a faint tick. A series longer than the width is bucketed by
its maximum. `tooltips` is parallel to `values` and gives the hover text of each bar. The
widget keeps the newest 366 values.

| Prop | Type | Default |
| --- | --- | --- |
| `values` | array of number or `null` | At most 366 items |
| `maximum` | number | absent: the largest value. Dropped unless above `0` |
| `highlightLast` | boolean | `true`: older bars are dimmed |
| `label` | string | Accessible label |
| `tooltips` | array of string or `null` | At most 366 items, parallel to `values` |
| `className` | class | |

### `sparkline`

A small line chart of up to 366 values. A leaf. A line through the values, scaled to their range. A `null` value breaks the line and
keeps its place on the horizontal axis; an isolated value is a dot. The widget keeps the newest
366 values.

| Prop | Type | Default |
| --- | --- | --- |
| `values` | array of number or `null` | At most 366 items |
| `fill` | boolean | `false`: shade the area under the line |
| `showLast` | boolean | `false`: dot on the last value |
| `label` | string | Accessible label |
| `className` | class | |

### `separator`

A line between groups. A leaf.

| Prop | Type | Default |
| --- | --- | --- |
| `orientation` | `horizontal`, `vertical` | `horizontal` |
| `className` | class | |

### `unsupported`

Any other `type`. No children. The name shown is the cleaned type string. Its props are
not stored.
