# Applet protocol

The panel is the host. An applet is a child process. They exchange UTF-8 NDJSON on the
child's stdin and stdout: one JSON object per line, separated by `\n`. The host also
accepts a trailing `\r`. The panel discards stderr; it is not part of the protocol.

Each line is at most 1 MiB (1,048,576 bytes), not counting the newline. A longer line is
rejected. A single JSON value nests at most 128 levels; a deeper tree is built across
commits. The message values `t` and `op` are a closed set. An unknown value, or a line
that is not JSON, does not decode.

The applet speaks first. The host does not send anything until it has read `hello`.

The only version is `1`, carried as `v` on both hellos.

Object keys added by a newer peer are ignored. A commit is applied as a whole batch or
not at all: if any operation in the batch is rejected, the tree does not change. The host
then stops that applet. The caps below are what the validator enforces, except the commit
rate, which the host counts itself.

## Host to applet

Every host message is an object with a `t` tag.

### `hello`

Sent in answer to the applet's `hello`, and again after the applet says `hello` on a
reload.

| Field | Type | Meaning |
| --- | --- | --- |
| `v` | number | `1` |
| `name` | string | This instance's name |
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
| `seq` | number or `null` | A number on `onChange`, otherwise `null` |

`seq` is always present. It is a number only for `onChange`. The applet copies that
number onto the next `set` of the same element when the set confirms the edited value.
A `set` with no `seq` is a deliberate replacement and is stored as sent. The host keeps
a newer local edit on screen when a later `set` of `value` carries an older `seq`; the
stored tree still records that set.

| Handler | Elements | `args` |
| --- | --- | --- |
| `onPress` | indicator | `[button]` — pointer button code: `1` left, `2` middle, `3` right, otherwise the code as reported |
| `onScroll` | indicator | `[dx, dy]` — scroll deltas, numbers |
| `onActivate` | row | `[]` |
| `onToggle` | switchrow, switch | `[active]` — boolean |
| `onChange` | fader, scale | `[value]` — number. `seq` is a number |
| `onMute` | fader | `[]` |
| `onChange` | entry | `[value]` — string. `seq` is a number |
| `onSubmit` | entry | `[value]` — string |
| `onClick` | button | `[]` |

A handler prop on an element is a boolean. `"onPress": true` subscribes. A missing or
`false` handler does not.

### `popover`

| Field | Type | Meaning |
| --- | --- | --- |
| `open` | boolean | Whether this instance's popover is showing |

## Applet to host

Every applet message is an object with a `t` tag.

### `hello`

| Field | Type |
| --- | --- |
| `v` | number, `1` |

The first message on stdout. The host answers with its own `hello`. A later `hello` from
the same process starts a new generation and an empty tree.

### `commit`

| Field | Type |
| --- | --- |
| `ops` | array of operations |

Operations are applied in order to a copy of the tree. Ids, cycles, the parent table,
the node cap and the child cap are checked before the copy replaces the live tree.

### `notify`

| Field | Type | Default |
| --- | --- | --- |
| `summary` | string | required |
| `body` | string | `""` |
| `icon` | string or absent | absent |
| `urgency` | `low`, `normal`, `critical` | `normal` |

### `copy`

| Field | Type |
| --- | --- |
| `text` | string |

### `open-uri`

| Field | Type |
| --- | --- |
| `uri` | string |

### `session`

| Field | Type |
| --- | --- |
| `action` | `lock`, `suspend`, `hibernate`, `log-out`, `reboot`, `power-off` |

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
`[]`. The whole subtree is new ids. Text of a `label`, `button` or `progress` is the
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
absent the stored seq stays.

A key whose value has the wrong type is dropped, and the other keys still apply. The
host logs each dropped key once per element. A number that is not finite is the same
kind of drop. JSON itself cannot carry `NaN` or infinity; a non-finite number is still
rejected if one is presented to the decoder.

## Parent table

The root holds any number of `indicator` elements and at most one `popover`.

| Parent | Children |
| --- | --- |
| root | `indicator`, and at most one `popover` |
| `popover` | at most one `hero`, at most one `footer`, and any body element |
| `section` | body elements except `section` |
| `box` | body elements except `section` |
| `footer` | `button`, `row`, `label`, `box` |
| anything else | nothing |

Body elements are `section`, `row`, `switchrow`, `fader`, `entry`, `placeholder`, `box`,
`label`, `image`, `button`, `switch`, `scale`, `spinner`, `progress`, `separator` and
`unsupported`.

An unknown `type` is `unsupported`. Its name is the type string after text cleaning,
capped at 64 characters. Markup in that name is kept literal.

A second `popover`, `hero` or `footer` where one is already held is rejected. So is a
child on a leaf, a `section` inside a `section` or a `box`, and any pair the table does
not list.

## Elements

`type` is one of the lowercase names below. Props use camelCase. Every element takes
`className`, one class from this list, or the key is dropped:

`dim-label`, `caption`, `heading`, `title-1`, `title-2`, `title-3`, `title-4`,
`numeric`, `accent`, `success`, `warning`, `error`, `flat`, `pill`, `circular`.

There is no applet CSS.

Unless a row below says otherwise:

- Strings that are labels, titles, tooltips, subtitles, descriptions, counts or
  placeholders go through text cleaning and a cap of 256 characters. Cleaning drops
  control characters and bidi overrides, folds whitespace, trims, and appends `…` when
  the cap cuts the string. An empty result removes the prop.
- `icon` and `overlay` must match `^[A-Za-z0-9_.-]+$`. Anything else is dropped.
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

At most one, under the root.

| Prop | Type |
| --- | --- |
| `className` | class |

Children follow the parent table. The order of `children` is the order on screen.

### `hero`

At most one, and only under `popover`. No children.

| Prop | Type |
| --- | --- |
| `icon` | string |
| `title` | string |
| `subtitle` | string |
| `className` | class |

### `section`

A body element. It holds body elements except another `section`.

| Prop | Type |
| --- | --- |
| `title` | string |
| `count` | string |
| `className` | class |

### `row`

A leaf.

| Prop | Type | Notes |
| --- | --- | --- |
| `icon` | string | |
| `title` | string | |
| `subtitle` | string | |
| `value` | string | |
| `selected` | boolean or absent | Present, either way, means the row can be selected |
| `busy` | boolean | |
| `className` | class | |
| `onActivate` | boolean | `false` or absent means the row does not activate |

### `switchrow`

A leaf.

| Prop | Type |
| --- | --- |
| `icon` | string |
| `title` | string |
| `subtitle` | string |
| `active` | boolean |
| `busy` | boolean |
| `className` | class |
| `onToggle` | boolean |

### `fader`

A leaf. `onMute: false` means the mute control is not shown.

| Prop | Type | Default |
| --- | --- | --- |
| `icon` | string | absent |
| `value` | number | `0` |
| `maximum` | number | `100`. Raised to `floor` when it would be lower |
| `floor` | number | `0`. Raised to `0` when negative |
| `muted` | boolean | `false` |
| `className` | class | |
| `onChange` | boolean | |
| `onMute` | boolean | |

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

### `footer`

At most one, and only under `popover`. Holds `button`, `row`, `label` and `box`.

| Prop | Type |
| --- | --- |
| `className` | class |

### `box`

A body element. Holds body elements except `section`.

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

### `progress`

A leaf. The caption is the `text` prop.

| Prop | Type | Default |
| --- | --- | --- |
| `fraction` | number | `0`, clamped to `0`–`1` |
| `text` | string | absent |
| `className` | class | |

### `separator`

A leaf.

| Prop | Type | Default |
| --- | --- | --- |
| `orientation` | `horizontal`, `vertical` | `horizontal` |
| `className` | class | |

### `unsupported`

Any other `type`. No children. The name shown is the cleaned type string. Props are
kept for a later `set` but the element carries only the name.

## Caps

| Cap | Value | Who enforces it |
| --- | --- | --- |
| Line length | 1,048,576 bytes | The host, while framing |
| JSON nesting | 128 levels in one value | The decoder |
| Nodes | 300, counting the root | The commit |
| Children of one parent | 200 | The commit |
| Hello and Commit updates | 120 per second combined | The host, not the tree |
| Text | 256 characters | The commit, via cleaning |
| Element name | 64 characters | The commit, via cleaning |
| `badge` | 8 characters | The commit, via cleaning |
| `entry.value` | 4,096 characters | The commit, without trimming |
| `dot` | 256 characters | The commit, without cleaning |

## Rejection

A batch is rejected, and the previous tree kept, when it:

- names an id that is not in the tree, or reuses one
- moves an element under itself or under a descendant
- puts a child where the parent table does not allow it, including a second `popover`,
  `hero` or `footer`
- would hold more than 300 nodes
- would give one parent more than 200 children
- tries to `set`, `move` or `remove` the root

`before` that is not a child of `parent` is the same kind of rejection. A line that does
not decode never reaches this list.
