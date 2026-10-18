# Applet protocol

The panel is the host. It starts every exec applet as a systemd user unit, including one that
`glimpsectl applet run` attaches. The applet connects once to the unix socket named
by `GLIMPSE_APPLET_SOCKET`; both directions carry UTF-8 NDJSON on that one connection: one JSON
object per line, separated by `\n`. Stdio carries the same stream in tests. The host also
accepts a trailing `\r`. Stderr is not part of the protocol; it goes to the journal, or to the
terminal of the `glimpsectl applet run` that attached the applet.

Each line is at most 1 MiB (1,048,576 bytes), not counting the newline. A longer line is
rejected. A single JSON value nests at most 128 levels; a deeper tree is built across
commits. The operation value `op` is a closed set: an unknown `op`, or a line that is not
JSON, does not decode. A message with an unknown `t` is ignored.

The applet speaks first. The host does not send anything until it has read `hello`.

The only version is `1`, carried as `v` on both hellos.

Object keys added by a newer peer are ignored. A commit is applied as a whole batch or
not at all: if any operation in the batch is rejected, the tree does not change. The host
then stops that applet. The caps below are what the validator enforces, except the line
rate, which the host counts itself: past the cap it stops reading until the next second,
so the applet blocks on its pipe. More than 5 `hello`s within any 10 seconds stops the applet
with the reason `too many hellos`, and it restarts like any stopped applet.

## Host to applet

Every host message is an object with a `t` tag.

### `hello`

Sent in answer to the applet's `hello`, and again after the applet says `hello` on a
reload.

| Field | Type | Meaning |
| --- | --- | --- |
| `v` | number | `1` |
| `name` | string | This instance's slot config key |
| `state` | string | Absolute path to the applet's persistent state directory, owned by the applet and mode 0700 |
| `entry` | object or null | The desktop entry's `id`, `name`, `icon` |
| `options` | object | The instance's options, a JSON object |
| `placement` | placement | Where this instance is drawn |

### `options`

Sent when the instance's options change. The process is not restarted.

| Field | Type |
| --- | --- |
| `options` | object |

### `placement`

Sent when the bar's placement changes and the process stays up.

| Field | Type |
| --- | --- |
| `placement` | placement |

A placement object:

| Field | Type | Values |
| --- | --- | --- |
| `output` | string or `null` | Connector name, such as `"DP-2"`. `null` when the bar has none |
| `position` | string | `top`, `bottom`, `left`, `right` — which edge the bar sits on |
| `orientation` | string | `horizontal`, `vertical` — how the bar lays out |
| `zone` | string | `left`, `center`, `right` |
| `size` | number | The bar's thickness, in logical pixels |

### `event`

A subscribed handler fired.

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | number | The element id |
| `name` | string | The handler name, below |
| `args` | array | The handler's arguments, in order. `[]` when it takes none |
| `seq` | number or `null` | A number on `onChange` and `onToggle`, otherwise `null` |

`seq` is always present. It is a number only for `onChange` and `onToggle`. The applet copies that
number onto the next `set` of the same element when the set confirms the edited value.
A `set` with no `seq` is a deliberate replacement and is stored as sent. The host keeps
a newer local edit on screen when a later `set` of the element's controlled prop
(`value`, `active`, `open` or `selected`) carries an older `seq`; the stored
tree still records that set.

| Handler | Elements | `args` |
| --- | --- | --- |
| `onPress` | indicator | `[button]` — pointer button code: `1` left, `2` middle, `3` right, otherwise the code as reported |
| `onScroll` | indicator | `[dx, dy]` — scroll deltas, numbers |
| `onActivate` | actionrow | `[]` |
| `onToggle` | switch, togglerow, expanderrow | `[active]` or `[open]` — boolean. `seq` is a number |
| `onMove` | sliderrow | `[value]` — number during a drag. `seq` is `null` |
| `onChange` | scale, sliderrow | `[value]` — number. `seq` is a number |
| `onChange` | entry | `[value]` — string. `seq` is a number |
| `onSubmit` | entry | `[value]` — string |
| `onClick` | button | `[]` |
| `onSelect` | choicelist, dropdown | `[index]` — number. `seq` is a number |
| `onSelect` | calendar | `[date]` — `YYYY-MM-DD` string |
| `onMonth` | calendar | `[year, month]` — numbers |
| `onActivate` | notice | `[]` |
| `onToggle` | togglebutton, checkbutton | `[active]` — boolean. `seq` is a number |

A handler prop on an element is a boolean. `"onPress": true` subscribes. A missing or
`false` handler does not.

The controlled props are `entry.value`, `scale.value`, `switch.active`, `togglebutton.active`,
`checkbutton.active`, `dropdown.selected`, `choicelist.selected`, `togglerow.active`,
`expanderrow.open` and `sliderrow.value`.
`sliderrow.onMove` reports motion without controlling `value`; `onChange` confirms the
value. An `expanderrow` closed by the panel through the accordion
or an outside press reports `onToggle(false)` with a bumped `seq`. An applet `set` of
`open` does not emit `onToggle`.

### `popover`

| Field | Type | Meaning |
| --- | --- | --- |
| `open` | boolean | Whether this instance's popover is showing |

`notify` and `copy` are SDK helpers that run `notify-send` and `wl-copy` from the applet process.

## Applet to host

Every applet message is an object with a `t` tag.

### `hello`

| Field | Type |
| --- | --- |
| `v` | number, `1` |

The first message the applet sends. The host answers with its own `hello`. A later `hello` from
the same process starts a new generation and an empty tree.

### `commit`

| Field | Type |
| --- | --- |
| `ops` | array of operations |

Operations are applied in order to a copy of the tree. Ids, cycles, the parent table,
the node cap and the child cap are checked before the copy replaces the live tree.

### `open-uri`

| Field | Type |
| --- | --- |
| `uri` | string |

Only `http`, `https`, `mailto` and `file` URIs of at most 2,048 bytes are opened; any other
is dropped. A `file` URI opens the resolved path when it is a directory, or a regular file that is
neither executable nor a type that starts a program: a desktop
entry, a script, a binary, an installer or an archive that runs.

### `launch`

| Field | Type |
| --- | --- |
| `id` | string — a desktop-file id, with or without `.desktop` |

### `close-popover`

No fields. `{"t":"close-popover"}`.

## Operations

An operation is an object with an `op` tag. Ids are integers the applet chooses. `0` is
the root and is never an element id. An id is unique for as long as its element exists.

`before` is required. `null` appends. A number is the sibling to insert in front of, and
that sibling must already be a child of `parent`.

### `insert`

```json
{"op":"insert","parent":0,"before":null,"node":{"id":1,"type":"indicator","props":{},"children":[]}}
```

| Field | Type |
| --- | --- |
| `parent` | number |
| `before` | number or `null` |
| `node` | element, with nested `children` |

`node` is `{id, type, props, children}`. `props` defaults to `{}` and `children` to
`[]`. The whole subtree is new ids. Text of a `label` or `button` is the
`text` prop, not a child node. The same is true of an indicator's label.

### `move`

```json
{"op":"move","parent":2,"id":5,"before":3}
```

Takes `id` off its current parent and inserts it under `parent`, before `before`.
A move under `id` itself, or under one of its descendants, is a cycle and rejects the
batch. Reordering is a move whose `parent` is already the parent.

### `remove`

```json
{"op":"remove","parent":1,"id":7}
```

`id` must be a child of `parent`. The element and every descendant are deleted.

### `set`

```json
{"op":"set","id":3,"props":{"title":"Sequenced"},"seq":4}
```

| Field | Type | Default |
| --- | --- | --- |
| `id` | number | required |
| `props` | object | required |
| `seq` | number or absent | absent |

`props` is a merge, not a replacement. A value of `null` deletes that key. Any other
key keeps its previous value. After the merge the element is derived again. The element
type does not change. When `seq` is present it is stored on the element; when it is
absent the stored seq is cleared.

A key the element does not take, or whose value has the wrong type, is dropped and not
stored, and the other keys still apply. A number that is not finite is the same kind of
drop. JSON itself cannot carry `NaN` or infinity; a non-finite number is still
rejected if one is presented to the decoder.

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
- Booleans default to `false`, except `sensitive`, which defaults to `true`.
- A missing object defaults each prop as the tables say.

### `indicator`

Only under the root. No children.

| Prop | Type | Notes |
| --- | --- | --- |
| `icon` | string | Icon name |
| `text` | string | Chip label |
| `tooltip` | string | |
| `badge` | string | Cleaned and capped at 8 characters |
| `overlay` | string | Icon name |
| `dot` | string | Stored as a string, capped at 256 characters, not parsed and not whitespace-folded. The panel reads it as a color |
| `severity` | `info`, `warning`, `error` | |
| `attention` | boolean | |
| `notice` | boolean | |
| `className` | class | |
| `onPress` | boolean | |
| `onScroll` | boolean | |

### `popover`

Under the root. `icon`, `title` and `subtitle` render the shell's header. The popover body
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

At most one, under `popover`. Its one child is `readout`, `switch`, `container` or `swatch`, drawn
in the header's end slot. It does not take `className`.

### `container`

A body element. At most one child, any body element except `section`. It does not take
`className`.

| Prop | Type | Notes |
| --- | --- | --- |
| `width` | number | Clamped to -1..=1024. -1 is the natural width |
| `height` | number | Clamped to -1..=1024. -1 is the natural height |
| `background` | paint | `red`, `orange`, `yellow`, `green`, `blue`, `purple`, `light`, `dark` |
| `color` | paint | The same paint names, applied to the text |
| `align` | align | `fill`, `start`, `end`, `center`, `baseline`. Applied to the child |

### `section`

A body element. It holds body elements except another `section`.

| Prop | Type |
| --- | --- |
| `title` | string |
| `count` | string |
| `className` | class |

### `row`

A body element. A static row: it fires no events. At most one end child: `image`, `readout`
or `spinner`.

| Prop | Type | Notes |
| --- | --- | --- |
| `icon` | string | Themed icon name |
| `title` | string | |
| `subtitle` | string | |
| `value` | string | |
| `tone` | `warning`, `error`, `destructive` | Absent is normal |
| `inactive` | boolean | `false`. Fades the row |
| `tooltip` | string | Plain text |
| `typography` | `numeric`, `monospace` | Absent is default |

Inactive action, toggle and meter rows are insensitive, leave Tab navigation and fade once.
Use `subtitle` to explain why a row is inactive. Backend `set` updates still render.

### `actionrow`

A body element. At most one end child: `image`, `readout` or `spinner`, and at most one
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

A body element. At most one end child: `image`, `readout` or `spinner`.
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
| `level` | number | Clamped to `0`–`1`. Absent or non-finite draws no bar |

### `expanderrow`

A body element. Its head may contain one `end` element with at most one
`image`, `readout` or `spinner` child, and one `start` element. Its other children fill the drawer as
listed in the parent table. `onToggle` sends `[open]` with a `seq` in every state.
Expansion is inspection, so the head stays activatable when inactive. Every drawer participates
in the accordion. `inactive` never propagates to drawer children.

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

Only under `expanderrow`. It takes no props and holds at most one `image`,
`readout` or `spinner`, drawn in the head's end slot. No events.

### `start`

Under `sliderrow`, `actionrow` or `expanderrow`. It takes no props and holds at most one
`image`, `button`, `readout`, `spinner`, `dots` or `swatch`. Under `sliderrow` it is drawn left of the
scale; under a row it takes the icon's place, centred, and hides `icon`. Empty slots are hidden.
No events.

### `sliderrow`

Holds at most one `start`. `onMove` sends `[value]` during a drag with `seq: null`;
`onChange` sends `[value]` once at drag end or on a keyboard step, with a `seq`.
Bounds: `min` `0`, `max` `1`, `step` `0.01`; a `step` that is not greater than `0`
becomes `0.01`; `min >= max` resets both to `0` and `1`. A non-finite number is dropped and
takes its default. `value` is clamped to `min`–`max`; a missing `value` is `min`. A `set`
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

A leaf. `value` is not text-cleaned. Control characters and bidi overrides are stripped
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

A leaf.

| Prop | Type |
| --- | --- |
| `icon` | string |
| `title` | string |
| `description` | string |
| `className` | class |

### `box`

A body element. Holds body elements.

| Prop | Type | Default |
| --- | --- | --- |
| `orientation` | `horizontal`, `vertical` | `horizontal` |
| `spacing` | number | `0`, clamped to `0`–`24` |
| `homogeneous` | boolean | `false` |
| `halign`, `valign` | `fill`, `start`, `end`, `center`, `baseline` | absent, so the widget's own align is left alone |
| `hexpand`, `vexpand` | boolean | `false` |
| `className` | class | |

### `label`

A leaf. The words are the `text` prop, plain text, never markup.

| Prop | Type | Default |
| --- | --- | --- |
| `text` | string | absent |
| `wrap` | boolean | `false` |
| `xalign` | number | absent. When set, clamped to `0`–`1` |
| `ellipsize` | `none`, `start`, `middle`, `end` | absent |
| `lines` | number | `0`, meaning no extra line limit. Rounded and clamped to `0`–`8` |
| `className` | class | |

### `image`

A leaf. `icon` is a themed name, never a path.
When set, `tooltip` also provides the accessible name.

| Prop | Type | Default |
| --- | --- | --- |
| `icon` | string | absent |
| `tooltip` | string | absent |
| `pixelSize` | number | absent. When set, rounded and clamped to `8`–`64` |
| `className` | class | |

### `swatch`

A leaf: a chip filled with `color`. Only in `addon` and `start`.

| Prop | Type | Default |
| --- | --- | --- |
| `color` | string | absent, an empty chip. A CSS color — `#e0563f`, `rgb(224 86 63)`, a name; capped at 256 characters; one that does not parse draws nothing |
| `tooltip` | string | absent |
| `size` | `regular`, `large` | `regular`. `large` is the addon's hero size |

### `button`

A leaf. The label is the `text` prop.
For an icon-only button, `tooltip` also provides the accessible name.

| Prop | Type | Default |
| --- | --- | --- |
| `text` | string | absent |
| `icon` | string | absent |
| `tooltip` | string | absent |
| `sensitive` | boolean | `true` |
| `className` | class | |
| `onClick` | boolean | |

### `switch`

A leaf.

| Prop | Type | Default |
| --- | --- | --- |
| `active` | boolean | `false` |
| `sensitive` | boolean | `true` |
| `className` | class | |
| `onToggle` | boolean | |

### `scale`

A leaf. `min` must be less than `max`. When a set breaks that, both return to `0` and
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

A leaf. No props besides `className`.

### `factlist`

A leaf. No events.

| Prop | Type | Notes |
| --- | --- | --- |
| `facts` | array of `{label, value, warning?}` | At most 64 items |

### `choicelist`

A leaf. `onSelect` sends `[index]`, a number, with a `seq`.

| Prop | Type | Notes |
| --- | --- | --- |
| `choices` | array of `{label, detail?, icon?, warning?}` | At most 64 items |
| `selected` | number | |
| `className` | class | |
| `onSelect` | boolean | |

### `readout`

A leaf.

| Prop | Type |
| --- | --- |
| `value` | string |
| `unit` | string |
| `className` | class |

### `notice`

A leaf. `onActivate` makes it a button.

| Prop | Type |
| --- | --- |
| `icon` | string |
| `title` | string |
| `subtitle` | string |
| `severity` | `info`, `warning` or `error` |
| `className` | class |
| `onActivate` | boolean |

### `calendar`

A leaf. `today` and `selected` are `YYYY-MM-DD`; a date that is not a real day is dropped.
`onMonth` sends `[year, month]` when the user pages to another month, and `onSelect`
sends `["YYYY-MM-DD"]`. Neither fires for a state set by the applet.

| Prop | Type | Notes |
| --- | --- | --- |
| `year` | number | Clamped to `1`–`9999`. With `month`, the month shown |
| `month` | number | Clamped to `1`–`12` |
| `selected` | date | Absent clears the selection |
| `today` | date | |
| `events` | array of `{date, colors}` | At most 64 items. `colors` holds at most 64 tones from `accent`, `dim`, `success`, `warning`, `error`; the widget draws the first three |
| `className` | class | |
| `onMonth` | boolean | |
| `onSelect` | boolean | |

### `dots`

A leaf.

| Prop | Type | Notes |
| --- | --- | --- |
| `colors` | array of tone | At most 64 items; tones are `accent`, `dim`, `success`, `warning`, `error`. Resolved from the theme when the element is set |
| `uniform` | boolean | |
| `size` | number | Clamped to `2`–`16` |
| `max` | number | Clamped to `1`–`3` |
| `className` | class | |

### `dropdown`

A leaf. `onSelect` sends `[index]`, a number, with a `seq`.

| Prop | Type | Notes |
| --- | --- | --- |
| `options` | array of string | At most 64 items |
| `selected` | number | |
| `className` | class | |
| `onSelect` | boolean | |

### `togglebutton`

A leaf. `onToggle` sends `[active]` with a `seq`.

| Prop | Type |
| --- | --- |
| `label` | string |
| `icon` | string |
| `active` | boolean |
| `className` | class |
| `onToggle` | boolean |

### `checkbutton`

A leaf. `onToggle` sends `[active]` with a `seq`.

| Prop | Type |
| --- | --- |
| `label` | string |
| `active` | boolean |
| `className` | class |
| `onToggle` | boolean |

### `revealer`

Holds any number of body children, including none.

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

A leaf. A range drawn on a scale. Non-finite numbers are dropped; `min` must be below
`max` or the scale is `0`–`1`; `low`, `high` and `now` are clamped into it and `high`
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

A leaf. One bar per value, scaled to the largest value or to `maximum`. A `null`, negative or
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

A leaf. A line through the values, scaled to their range. A `null` value breaks the line and
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

A leaf.

| Prop | Type | Default |
| --- | --- | --- |
| `orientation` | `horizontal`, `vertical` | `horizontal` |
| `className` | class | |

### `unsupported`

Any other `type`. No children. The name shown is the cleaned type string. Its props are
not stored.

## Caps

| Cap | Value | Who enforces it |
| --- | --- | --- |
| Line length | 1,048,576 bytes | The host, while framing |
| JSON nesting | 128 levels in one value | The decoder |
| Nodes | 300, counting the root | The commit |
| Children of one parent | 200 | The commit |
| Operations per commit | 1,200 | The commit |
| Lines read | 120 per second; more are delayed, not rejected | The host, not the tree |
| Text | 256 characters | The commit, via cleaning |
| Element name | 64 characters | The commit, via cleaning |
| `badge` | 8 characters | The commit, via cleaning |
| `entry.value` | 4,096 characters | The commit, without trimming |
| `dot` | 256 characters | The commit, without cleaning |

## Rejection

A batch is rejected, and the previous tree kept, when it:

- names an id that is not in the tree, or reuses one
- moves an element under itself or under a descendant
- puts a child where the parent table does not allow it
- carries more than 1,200 operations
- would hold more than 300 nodes once the whole batch is applied
- would give one parent more than 200 children once the whole batch is applied
- tries to `set`, `move` or `remove` the root

`before` that is not a child of `parent` is the same kind of rejection. A line that does
not decode never reaches this list.
