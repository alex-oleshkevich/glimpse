# Applet protocol

Version 1 of the wire protocol between the panel and an exec applet: framing, every message in both
directions, the tree operations, and the limits the panel enforces. You need it to write an SDK; an
applet built on an SDK never sees it. The elements and their props are in
[Elements](/applets/exec/reference/elements).

## Transport and framing

The applet connects to `GLIMPSE_APPLET_SOCKET`; both directions carry UTF-8 NDJSON on that unix socket:
one JSON object per line, separated by `\n`. Stdio carries the same stream in tests. The host also
accepts a trailing `\r`. Stderr is not part of the protocol; it goes to the journal, or to the
terminal of the `glimpsectl applet run` that attached the applet.

Each line is at most 1 MiB (1,048,576 bytes), not counting the newline. A longer line is
rejected. A single JSON value nests at most 128 levels; a deeper tree is built across
commits. The operation value `op` is a closed set: an unknown `op`, or a line that is not
JSON, does not decode. A message with an unknown `t` is ignored.

The applet speaks first; both hellos carry `v: 1`.

## How one conversation goes

1. The applet connects to the unix socket in `GLIMPSE_APPLET_SOCKET`. Both directions then carry NDJSON on that one connection.
2. The applet sends `hello` with `v: 1`. The host starts a new generation and clears the protocol tree.
3. The host answers with its own `hello` (`name`, `state`, `options`, `placement`, `entry`), then sends `popover` if one is open.
4. The applet sends a `commit`, one batch of `ops` that builds or changes its widget tree.
5. The host applies the batch to a copy of the tree, checks the whole batch, and only then publishes it to the exec widget. A rejected batch changes nothing on screen.
6. A user gesture becomes an `event`. Events carry the generation of the tree they came from, and the host drops any event from an older generation.
7. The applet replies with another `commit`, or with a request such as `open-uri`.

Object keys added by a newer peer are ignored. A commit is applied as a whole batch or
not at all: if any operation in the batch is rejected, the tree does not change. The host
then stops that applet. The caps below are what the validator enforces, except the line
rate, which the host counts itself: past the cap it stops reading until the next second,
so the applet blocks on its socket. More than 5 `hello`s within any 10 seconds kills the unit
with the reason `too many hellos`. systemd restarts configured applets; development copies stay down.

## Host to applet

Every host message is an object with a `t` tag. Message shapes below are written as TypeScript
types; each is one JSON object on one line.

| `t` | Sent when |
| --- | --- |
| `hello` | In answer to the applet's `hello` |
| `options` | The instance's options changed |
| `placement` | The bar's placement changed |
| `event` | A subscribed element handler fired |
| `popover` | The instance's popover opened or closed |

### `hello`

Sent in answer to the applet's `hello`, and again after the applet says `hello` on a
reload.

```ts
type Hello = {
  t: "hello";
  v: 1;
  name: string;          // this instance's name
  state: string;         // absolute path to the applet's state directory, mode 0700
  options: object;       // the instance's options
  placement: Placement;  // where this instance is drawn
  entry?: { id: string; name: string; icon: string | null } | null; // the desktop entry
};
```

### `options`

Sent when `glimpsectl applet run` reloads the project's options. The process is not restarted; a
change to the panel configuration restarts it instead.

```ts
type Options = { t: "options"; options: object };
```

### `placement`

Sent when the bar's placement changes and the process stays up.

```ts
type PlacementMessage = { t: "placement"; placement: Placement };

type Placement = {
  output: string | null;                          // connector name such as "DP-2", or null
  position: "top" | "bottom" | "left" | "right";  // the edge the bar sits on
  orientation: "horizontal" | "vertical";         // how the bar lays out
  zone: "left" | "center" | "right";
  size: number;                                   // the bar's thickness, in logical pixels
};
```

### `event`

A subscribed handler fired.

```ts
type Event = {
  t: "event";
  id: number;          // the element id
  name: string;        // the handler name, below
  args: unknown[];     // the handler's arguments, in order; [] when it takes none
  seq: number | null;
};
```

`seq` is always present. It is a number only for `onChange`, `onToggle` and the choicelist and dropdown
`onSelect`. The applet copies that
number onto the next `set` of the same element when the set confirms the edited value.
A `set` with no `seq` is a deliberate replacement and is stored as sent. The host keeps
a newer local edit on screen when a later `set` of the element's controlled prop
(`value`, `active`, `open` or `selected`) carries an older `seq`; the stored
tree still records that set.

| Handler | Elements | `args` |
| --- | --- | --- |
| `onPress` | indicator | `[button]`: `2` middle, `3` right, otherwise the code as reported. A left click opens the popover and never fires `onPress` |
| `onScroll` | indicator | `[dx, dy]`: scroll deltas, numbers |
| `onActivate` | actionrow | `[]` |
| `onToggle` | switch, togglerow, expanderrow | `[active]` or `[open]`: boolean. `seq` is a number |
| `onMove` | sliderrow | `[value]`: number during a drag. `seq` is `null` |
| `onChange` | scale, sliderrow | `[value]`: number. `seq` is a number |
| `onChange` | entry, entryrow | `[value]`: string. `seq` is a number |
| `onSubmit` | entry, entryrow | `[value]`: string |
| `onClick` | button | `[]` |
| `onSelect` | choicelist, dropdown | `[index]`: number. `seq` is a number |
| `onSelect` | calendar | `[date]`: `YYYY-MM-DD` string |
| `onMonth` | calendar | `[year, month]`: numbers |
| `onActivate` | notice | `[]` |
| `onToggle` | togglebutton, checkbutton | `[active]`: boolean. `seq` is a number |

A handler prop on an element is a boolean. `"onPress": true` subscribes. A missing or
`false` handler does not.

The controlled props are `entry.value`, `entryrow.value`, `scale.value`, `switch.active`, `togglebutton.active`,
`checkbutton.active`, `dropdown.selected`, `choicelist.selected`, `togglerow.active`,
`expanderrow.open` and `sliderrow.value`.
`sliderrow.onMove` reports motion without controlling `value`; `onChange` confirms the
value. An `expanderrow` closed by the panel through the accordion
or an outside press reports `onToggle(false)` with a bumped `seq`. An applet `set` of
`open` does not emit `onToggle`.

### `popover`

```ts
type Popover = { t: "popover"; open: boolean }; // whether this instance's popover is showing
```

## Applet to host

Every applet message is an object with a `t` tag.

| `t` | Meaning |
| --- | --- |
| `hello` | First message on a connection; starts a new generation |
| `commit` | A batch of tree operations |
| `open-uri` | Ask the host to open a URI |
| `launch` | Ask the host to launch a desktop entry |
| `close-popover` | Close this instance's popover |

### `hello`

```ts
type Hello = { t: "hello"; v: 1 };
```

The first message the applet sends. The host answers with its own `hello`. A later `hello` from
the same process starts a new generation and an empty tree.

### `commit`

```ts
type Commit = { t: "commit"; ops: Operation[] };
```

Operations are applied in order to a copy of the tree. Ids, cycles, the [parent table](/applets/exec/reference/elements#parent-table),
the node cap and the child cap are checked before the copy replaces the live tree.

### `open-uri`

```ts
type OpenUri = { t: "open-uri"; uri: string };
```

Only `http`, `https`, `mailto` and `file` URIs of at most 2,048 bytes are opened; any other
is dropped. A `file` URI opens the resolved path when it is a directory, or a regular file that is
neither executable nor a type that starts a program: a desktop
entry, a script, a binary, an installer or an archive that runs.

### `launch`

```ts
type Launch = { t: "launch"; id: string }; // a desktop-file id, with or without .desktop
```

### `close-popover`

No fields. `{"t":"close-popover"}`.

## Operations

An operation is an object with an `op` tag. Ids are integers the applet chooses. `0` is
the root and is never an element id. An id is unique for as long as its element exists.

`before` may be absent or `null`; either appends. A number is the sibling to insert in front of, and
that sibling must already be a child of `parent`.

### `insert`

```json
{"op":"insert","parent":0,"before":null,"node":{"id":1,"type":"indicator","props":{},"children":[]}}
```

```ts
type Insert = { op: "insert"; parent: number; before?: number | null; node: Element };
type Element = { id: number; type: string; props?: object; children?: Element[] };
```

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

```ts
type Set = { op: "set"; id: number; props: object; seq?: number };
```

`props` is a merge, not a replacement. A value of `null` deletes that key. Any other
key keeps its previous value. After the merge the element is derived again. The element
type does not change. When `seq` is present it is stored on the element; when it is
absent the stored seq is cleared.

A key the element does not take, or whose value has the wrong type, is dropped and not
stored, and the other keys still apply. A number that is not finite is the same kind of
drop. JSON itself cannot carry `NaN` or infinity; a non-finite number is still
rejected if one is presented to the decoder.

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
| `entry.value`, `entryrow.value` | 4,096 characters | The commit, without trimming |
| `dot` | 256 characters | The commit, without cleaning |

## Rejection

A batch is rejected, and the previous tree kept, when it:

- names an id that is not in the tree, or reuses one
- moves an element under itself or under a descendant
- puts a child where the [parent table](/applets/exec/reference/elements#parent-table) does not allow it
- carries more than 1,200 operations
- would hold more than 300 nodes once the whole batch is applied
- would give one parent more than 200 children once the whole batch is applied
- tries to `set`, `move` or `remove` the root
- carries an array prop over its item cap (64, or 366 for chart `values` and `tooltips`)
- gives an element more children of a kind than its table allows, such as a second `addon`

`before` that is not a child of `parent` is the same kind of rejection. A line that does
not decode never reaches this list.
