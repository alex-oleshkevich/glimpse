# SDK API

The `glimpse-sdk` package exports React components, hooks, host requests, and supporting types for exec applets. Configure TypeScript with `"jsx": "react-jsx"` and `"jsxImportSource": "glimpse-sdk"`.

## Component props

Every component accepts the React `key?: string | number` attribute. All props below are optional except `SliderRow.value`. `children` has type `Node`. The notation `string`, `number`, and `boolean` means the corresponding TypeScript type; quoted values are string literals.

### Shared types

```ts
type Tone = "accent" | "dim" | "success" | "warning" | "error";
type Align = "fill" | "start" | "end" | "center" | "baseline";
type PopoverSize = "regular" | "wide";
type Paint = "red" | "orange" | "yellow" | "green" | "blue" | "purple" | "light" | "dark";
type RowTone = "warning" | "error" | "destructive";
type RowTypography = "numeric" | "monospace";
type ClassName =
  | "dim-label" | "caption" | "heading"
  | "title-1" | "title-2" | "title-3" | "title-4" | "numeric"
  | "accent" | "success" | "warning" | "error"
  | "flat" | "pill" | "circular" | "column"
  | "network-popover__rule" | "bluetooth-popover" | "network-popover";
```

`className` takes one `ClassName`, not a space-separated class list. There is no applet CSS.

### Indicator and popover

| Component | Props |
| --- | --- |
| `Indicator` | `icon?: string`, `tooltip?: string`, `text?: string`, `badge?: string`, `overlay?: string`, `dot?: string`, `severity?: "info" \| "warning" \| "error"`, `attention?: boolean`, `notice?: boolean`, `className?: ClassName`, `onPress?: (button: number) => void`, `onScroll?: (dx: number, dy: number) => void`, `children?: Node` |
| `Popover` | `className?: ClassName`, `icon?: string`, `title?: string`, `subtitle?: string`, `size?: PopoverSize`, `children?: Node` |
| `Addon` | `children?: Node` |
| `Container` | `width?: number`, `height?: number`, `background?: Paint`, `color?: Paint`, `align?: Align`, `children?: Node` |
| `Section` | `title?: string`, `count?: string`, `className?: ClassName`, `children?: Node` |

An `Indicator`'s `onPress` receives `2` for middle and `3` for right, or the raw button code for any other button. A left click opens the popover and never fires `onPress`. `onScroll` receives numeric deltas. Text children of `Indicator`, `Label`, and `Button` supply their displayed text. The root accepts indicators and popovers; the panel shows the first popover.

`dot` takes a CSS color such as `"#e01b24"`; an unparseable value shows no dot. `attention` shows a dot while no badge is visible. `notice` gives the icon and label a rounded accent background. `severity="warning"` and `severity="error"` style the chip; `"info"` looks the same as no severity.

### Rows

The following base props apply to every component in the next table unless stated otherwise:

| Prop | Type |
| --- | --- |
| `icon`, `title`, `subtitle`, `value`, `tooltip` | `string` |
| `tone` | `RowTone` |
| `inactive`, `busy` | `boolean` |
| `typography` | `RowTypography` |

| Component | Base props and additional props |
| --- | --- |
| `Row` | Base props except `busy`; `children?: Node` |
| `ActionRow` | Base props; `onActivate?: () => void`, `children?: Node` |
| `ToggleRow` | Base props; `control?: "check" \| "switch"`, `active?: boolean`, `onToggle?: (active: boolean) => void` |
| `MeterRow` | Base props; `level?: number`, `children?: Node` |
| `ExpanderRow` | Base props; `open?: boolean`, `tint?: boolean`, `titleLines?: number`, `onToggle?: (open: boolean) => void`, `children?: Node` |
| `SliderRow` | Does not use the base props. `title?: string`, `subtitle?: string`, `value: number`, `min?: number`, `max?: number`, `step?: number`, `tone?: RowTone`, `inactive?: boolean`, `tooltip?: string`, `onMove?: (value: number) => void`, `onChange?: (value: number) => void`, `children?: Node` |
| `Start` | `children?: Node` |
| `End` | `children?: Node` |

`MeterRow.level` is a fraction from `0` to `1`. `SliderRow.onMove` reports dragging; `onChange` reports release or a keyboard step. Inactive action, toggle, and meter rows suppress primary input. An inactive expander can still expand for inspection.

`ExpanderRow.titleLines` defaults to `1` and is limited to `1` through `4`. `SliderRow.step` defaults to `0.01`.

### Inputs and actions

| Component | Props |
| --- | --- |
| `Entry` | `placeholder?: string`, `value?: string`, `className?: ClassName`, `onChange?: (value: string) => void`, `onSubmit?: (value: string) => void` |
| `Button` | `text?: string`, `icon?: string`, `tooltip?: string`, `sensitive?: boolean`, `className?: ClassName`, `onClick?: () => void`, `children?: Node` |
| `Switch` | `active?: boolean`, `sensitive?: boolean`, `className?: ClassName`, `onToggle?: (active: boolean) => void` |
| `Scale` | `value?: number`, `min?: number`, `max?: number`, `step?: number`, `sensitive?: boolean`, `className?: ClassName`, `onChange?: (value: number) => void` |
| `ToggleButton` | `label?: string`, `icon?: string`, `active?: boolean`, `className?: ClassName`, `onToggle?: (active: boolean) => void` |
| `CheckButton` | `label?: string`, `active?: boolean`, `className?: ClassName`, `onToggle?: (active: boolean) => void` |
| `DropDown` | `options?: string[]`, `selected?: number`, `className?: ClassName`, `onSelect?: (index: number) => void` |
| `ChoiceList` | `choices?: { label: string; detail?: string; icon?: string; warning?: boolean }[]`, `selected?: number`, `className?: ClassName`, `onSelect?: (index: number) => void` |
| `Calendar` | `year?: number`, `month?: number`, `selected?: string`, `today?: string`, `events?: { date: string; colors?: Tone[] }[]`, `className?: ClassName`, `onMonth?: (year: number, month: number) => void`, `onSelect?: (date: string) => void` |
| `Notice` | `icon?: string`, `title?: string`, `subtitle?: string`, `severity?: "info" \| "warning" \| "error"`, `className?: ClassName`, `onActivate?: () => void` |

Calendar dates use `YYYY-MM-DD`; the host clamps months to `1` through `12` and years to `1` through `9999`. Selection indexes are numbers. Controlled inputs need updated props in response to their callbacks. `Notice.onActivate` makes the notice a button. Give icon-only buttons a tooltip for their accessible name.

### Layout and display

| Component | Props |
| --- | --- |
| `Box` | `orientation?: "horizontal" \| "vertical"`, `spacing?: number`, `homogeneous?: boolean`, `halign?: Align`, `valign?: Align`, `hexpand?: boolean`, `vexpand?: boolean`, `className?: ClassName`, `children?: Node` |
| `Label` | `text?: string`, `wrap?: boolean`, `xalign?: number`, `ellipsize?: "none" \| "start" \| "middle" \| "end"`, `lines?: number`, `className?: ClassName`, `children?: Node` |
| `Image` | `icon?: string`, `tooltip?: string`, `pixelSize?: number`, `className?: ClassName` |
| `Swatch` | `color?: string`, `tooltip?: string`, `size?: "regular" \| "large"` |
| `Placeholder` | `icon?: string`, `title?: string`, `description?: string`, `className?: ClassName` |
| `Spinner` | `className?: ClassName` |
| `Separator` | `orientation?: "horizontal" \| "vertical"`, `className?: ClassName` |
| `FactList` | `facts?: { label: string; value: string; warning?: boolean }[]` |
| `Readout` | `value?: string`, `unit?: string`, `className?: ClassName` |
| `Dots` | `colors?: Tone[]`, `uniform?: boolean`, `size?: number`, `max?: number`, `className?: ClassName` |
| `Revealer` | `revealed?: boolean`, `transition?: "slide-down" \| "crossfade" \| "none"`, `className?: ClassName`, `children?: Node` |
| `Stack` | `visible?: number`, `className?: ClassName`, `children?: Node` |
| `RangeBar` | `min?: number`, `max?: number`, `low?: number`, `high?: number`, `now?: number`, `className?: ClassName` |
| `Bars` | `values?: Array<number \| null>`, `maximum?: number`, `highlightLast?: boolean`, `label?: string`, `tooltips?: Array<string \| null>`, `className?: ClassName` |
| `Sparkline` | `values?: Array<number \| null>`, `fill?: boolean`, `showLast?: boolean`, `label?: string`, `className?: ClassName` |

`Label` displays plain text, never markup. `Image.icon` is a themed icon name, not a path. `Swatch.color` is a CSS color. `Stack.visible` selects a child by index. `Bars` and `Sparkline` accept `null` for missing data and retain at most 366 values.

All icon props accept only ASCII letters, digits, `_`, `.` and `-`. A name with any other character is dropped, leaving the widget without that icon.

The host clamps `Container.width` and `height` to `-1` through `1024`, `Box.spacing` to `0` through `24`, and `Label.xalign` to `0` through `1`. `Label.lines` accepts nonnegative values, rounds them, and caps them at `8`. `Image.pixelSize` is rounded and clamped to `8` through `64`. `Dots.size` is clamped to `2` through `16`, and `Dots.max` to `1` through `3`.

`SliderRow`, `Scale`, and `RangeBar` default to `min=0` and `max=1`. If `min >= max`, the host resets that range to `0` through `1`. Slider values and `RangeBar.low` and `now` are clamped to the range; `RangeBar.high` is clamped between `low` and `max`.

### Nesting and size limits

JSX types permit some children that the host rejects. Text children on `Indicator`, `Label`, and `Button` become text props, not child widgets. Other children must follow these rules:

| Parent | Allowed children |
| --- | --- |
| Root | `Indicator`, `Popover` |
| `Popover` | At most one `Addon`, plus body elements |
| `Addon` | At most one `Readout`, `Switch`, `Container`, or `Swatch` |
| `Container` | At most one body element except `Section` |
| `Section`, `Revealer`, `Stack` | Body elements except `Section` |
| `Box` | Body elements |
| `Row`, `MeterRow` | At most one `Image`, `Readout`, or `Spinner` |
| `ActionRow` | At most one `Image`, `Readout`, or `Spinner`, plus at most one `Start` |
| `ExpanderRow` | At most one `End` and one `Start`; drawer children are `ActionRow`, `ToggleRow`, `MeterRow`, `SliderRow`, `ExpanderRow`, `FactList`, `ChoiceList`, or `Row` |
| `End` | At most one `Image`, `Readout`, or `Spinner` |
| `Start` | At most one `Image`, `Button`, `Readout`, `Spinner`, `Dots`, or `Swatch` |
| `SliderRow` | At most one `Start` |
| Other components | No child widgets |

Body elements are the exported components other than `Indicator`, `Popover`, `Addon`, `Start`, `End`, and `Swatch`.

The host allows 300 nodes including the root, 200 children per parent, and 1,200 operations per commit. An array prop longer than 64 items, at any depth, is rejected rather than shortened; chart `values` and `tooltips` allow 366. Most text is cleaned and capped at 256 characters, badges at eight, the indicator label on the chip at 32, and entry values at 4,096. A rejected commit stops the applet.

## Hooks

| Hook | Signature and result |
| --- | --- |
| `useState` | `useState<T>(initial: T \| (() => T)): [T, (action: T \| ((prev: T) => T)) => void]`. React state. |
| `useEffect` | `useEffect(effect: () => void \| (() => void), deps?: readonly unknown[]): void`. React effect and optional cleanup. |
| `useOptions` | `useOptions<T extends Record<string, unknown> = Record<string, unknown>>(): T`. Current options; updates without a restart. The generic is a cast, not runtime validation. |
| `usePlacement` | `usePlacement(): Placement \| null`. Current placement; `null` before it is supplied. |
| `usePopoverOpen` | `usePopoverOpen(): boolean`. Whether the popover is open. |
| `useAppletName` | `useAppletName(): string`. This configured instance's name. |
| `useStateDir` | `useStateDir(): string`. Host-provided persistent directory, keyed by desktop id and created with mode `0700`. |

## Desktop actions

| Function | Signature |
| --- | --- |
| `notify` | `notify(message: { summary: string; body?: string; icon?: string; urgency?: "low" \| "normal" \| "critical" }): void` |
| `copy` | `copy(text: string): void` |
| `openUri` | `openUri(uri: string): void` |
| `launch` | `launch(id: string): void` |
| `closePopover` | `closePopover(): void` |

`notify` defaults to an empty body and normal urgency. It runs `notify-send` from the applet process and `copy` runs `wl-copy`, so libnotify and wl-clipboard must be installed; the notification daemon applies its own limits. `launch` takes an installed desktop-file id with or without `.desktop`. `openUri` accepts only `http`, `https`, `mailto`, and eligible `file` URIs, at most 2,048 bytes. See [host requests and limits](/applets/exec/host) for file restrictions.

## Runtime and JSX exports

| Export | Type or signature |
| --- | --- |
| `run` | `run(element: unknown, io?: Io): Promise<void>` |
| `createElement` | `createElement(type: unknown, props?: Record<string, unknown> \| null, ...children: unknown[]): unknown` |
| `Fragment` | React's `Fragment`, re-exported for grouping siblings without a host node. |
| `jsx`, `jsxs`, `jsxDEV` | Each has signature `(type: unknown, props: unknown, key?: unknown) => unknown`; used by the JSX runtime. |

`run` uses the socket named by `GLIMPSE_APPLET_SOCKET`, or stdin/stdout when that variable is absent. It redirects console methods to stderr and exits with code zero when the session ends normally. A supplied `Io` replaces the default transport.

```ts
type Io = {
  writeLine: (line: string) => void;
  lines: AsyncIterable<string | string[]>;
};

type Placement = {
  output: string | null;
  position: string;
  orientation: string;
  zone: string;
  size: number;
};

type Node =
  | string | number | boolean | null | undefined | Node[]
  | { type: unknown; props: unknown; key: unknown };

type FC<P = object> = (props: P & { key?: string | number }) => Node;
```

The host's placement values are `top`, `bottom`, `left`, or `right` for `position`; `horizontal` or `vertical` for `orientation`; and `left`, `center`, or `right` for `zone`. `size` is the bar's logical-pixel thickness, and `output` is its connector name or `null`.

The separate `glimpse-sdk/testing` entry point exports `render` and the `Rendered` type. See [Test an applet](/applets/exec/testing) for its methods.
