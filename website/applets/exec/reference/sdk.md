# TypeScript SDK

The `glimpse-sdk` package exports React components, hooks, host requests, and supporting types for exec applets. Configure TypeScript with `"jsx": "react-jsx"` and `"jsxImportSource": "glimpse-sdk"`.

## Components

Every component accepts React's `key?: string | number`. A prop marked `?` is optional; only
`SliderRow.value` is required. The interface names are for reading: the package exports the
components, and `React.ComponentProps<typeof Indicator>` gives a component's props type. Which
component may hold which is in [Elements](/applets/exec/reference/elements#parent-table); a
rejected commit stops the applet.

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

`className` takes one `ClassName`, not a space-separated list. There is no applet CSS. Icon props
accept only ASCII letters, digits, `_`, `.` and `-`; any other name is dropped. Text props are
plain text, never markup, capped at 256 characters.

## Bar and popover

### Indicator

The chip on the bar. See [indicator](/applets/exec/reference/elements#indicator).

```ts
interface IndicatorProps {
  icon?: string;
  tooltip?: string;
  text?: string;
  badge?: string;
  overlay?: string;
  dot?: string;
  severity?: "info" | "warning" | "error";
  attention?: boolean;
  notice?: boolean;
  className?: ClassName;
  onPress?: (button: number) => void;
  onScroll?: (dx: number, dy: number) => void;
  children?: Node;     // text children become the label
}
```

```tsx
<Indicator
  icon="view-list-symbolic"
  text="3 left"
  tooltip="Todo"
  onPress={(button) => {
    if (button === 2) clearDone();
  }}
/>
```

### Popover

The card that opens on a left click. See [popover](/applets/exec/reference/elements#popover).

```ts
interface PopoverProps {
  className?: ClassName;
  icon?: string;
  title?: string;
  subtitle?: string;
  size?: PopoverSize;
  children?: Node;
}
```

```tsx
<Popover icon="view-list-symbolic" title="Todo" subtitle="3 left">
  <Section title="Today">
    <ToggleRow title="Buy groceries" control="check" active={done} onToggle={setDone} />
  </Section>
</Popover>
```

### Addon

One control in the popover header's end slot. See [addon](/applets/exec/reference/elements#addon).

```ts
interface AddonProps {
  children?: Node;
}
```

```tsx
<Popover title="Focus">
  <Addon>
    <Switch active={focused} onToggle={setFocused} />
  </Addon>
</Popover>
```

### Container

A sized, painted box around one child. See [container](/applets/exec/reference/elements#container).

```ts
interface ContainerProps {
  width?: number;
  height?: number;
  background?: Paint;
  color?: Paint;
  align?: Align;
  children?: Node;
}
```

```tsx
<Container width={160} background="blue" color="light" align="center">
  <Label text="Deep work" />
</Container>
```

### Section

A titled group of body elements. See [section](/applets/exec/reference/elements#section).

```ts
interface SectionProps {
  title?: string;
  count?: string;
  className?: ClassName;
  children?: Node;
}
```

```tsx
<Section title="Today" count="2">
  <ToggleRow title="Buy groceries" control="check" active={done} onToggle={setDone} />
  <ToggleRow title="Call the dentist" control="check" active={false} />
</Section>
```

## Rows

Rows share these props. See [row](/applets/exec/reference/elements#row).

```ts
interface RowProps {
  icon?: string;
  title?: string;
  subtitle?: string;
  value?: string;
  tone?: RowTone;
  inactive?: boolean;
  tooltip?: string;
  typography?: RowTypography;
  busy?: boolean;
}
```

### Row

A static row. See [row](/applets/exec/reference/elements#row).

```ts
interface PlainRowProps extends Omit<RowProps, "busy"> {
  children?: Node;
}
```

```tsx
<Row icon="network-wireless-symbolic" title="Lighthouse" value="5 GHz" typography="numeric" />
```

### ActionRow

A row you click. See [actionrow](/applets/exec/reference/elements#actionrow).

```ts
interface ActionRowProps extends RowProps {
  onActivate?: () => void;
  children?: Node;
}
```

```tsx
<ActionRow
  icon="folder-documents-symbolic"
  title="Open the project page"
  onActivate={() => openUri("https://github.com/alex-oleshkevich/glimpse")}
/>
```

### ToggleRow

A row with a switch or a check box. See [togglerow](/applets/exec/reference/elements#togglerow).

```ts
interface ToggleRowProps extends RowProps {
  control?: "check" | "switch"; // "switch" when absent
  active?: boolean;
  onToggle?: (active: boolean) => void;
}
```

```tsx
<ToggleRow title="Buy groceries" control="check" active={done} onToggle={setDone} />
```

### MeterRow

A row with a level bar. See [meterrow](/applets/exec/reference/elements#meterrow).

```ts
interface MeterRowProps extends RowProps {
  level?: number;
  children?: Node;
}
```

```tsx
<MeterRow icon="drive-harddisk-symbolic" title="Home" value="72%" level={0.72} />
```

### ExpanderRow

A row that opens a drawer of rows below it. See [expanderrow](/applets/exec/reference/elements#expanderrow).

```ts
interface ExpanderRowProps extends RowProps {
  open?: boolean;
  tint?: boolean;
  titleLines?: number;
  onToggle?: (open: boolean) => void;
  children?: Node;
}
```

```tsx
<ExpanderRow title="Work" open={open} onToggle={setOpen}>
  <End>
    <Readout value="2" unit="left" />
  </End>
  <ActionRow title="Write the report" onActivate={() => setDone(true)} />
  <ActionRow title="Review pull requests" onActivate={() => setDone(true)} />
</ExpanderRow>
```

### SliderRow

A row with a slider. See [sliderrow](/applets/exec/reference/elements#sliderrow).

```ts
interface SliderRowProps {
  title?: string;
  subtitle?: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  tone?: RowTone;
  inactive?: boolean;
  tooltip?: string;
  onMove?: (value: number) => void;
  onChange?: (value: number) => void;
  children?: Node;
}
```

```tsx
<SliderRow title="Volume" value={volume} onChange={setVolume}>
  <Start>
    <Image icon="audio-volume-high-symbolic" />
  </Start>
</SliderRow>
```

### Start and End

Slots in a row head, [start](/applets/exec/reference/elements#start) and [end](/applets/exec/reference/elements#end).

```ts
interface StartProps {
  children?: Node;
}

interface EndProps {
  children?: Node;
}
```

```tsx
<ActionRow title="Accent color" onActivate={() => setDone(true)}>
  <Start>
    <Swatch color="#3584e4" />
  </Start>
</ActionRow>
```

## Inputs

### Entry

A text field. See [entry](/applets/exec/reference/elements#entry).

```ts
interface EntryProps {
  placeholder?: string;
  value?: string;
  className?: ClassName;
  onChange?: (value: string) => void;
  onSubmit?: (value: string) => void;
}
```

```tsx
<Entry placeholder="New task" value={draft} onChange={setDraft} onSubmit={addTask} />
```

### Button

A push button. See [button](/applets/exec/reference/elements#button).

```ts
interface ButtonProps {
  text?: string;
  icon?: string;
  tooltip?: string;
  sensitive?: boolean; // true when absent
  className?: ClassName;
  onClick?: () => void;
  children?: Node;     // text children become the label
}
```

```tsx
<Button icon="list-add-symbolic" tooltip="Add a task" className="flat" onClick={() => addTask(draft)} />
```

### Switch

An on/off switch. See [switch](/applets/exec/reference/elements#switch).

```ts
interface SwitchProps {
  active?: boolean;
  sensitive?: boolean;
  className?: ClassName;
  onToggle?: (active: boolean) => void;
}
```

```tsx
<Switch active={focused} onToggle={setFocused} />
```

### Scale

A bare slider. See [scale](/applets/exec/reference/elements#scale).

```ts
interface ScaleProps {
  value?: number;
  min?: number;  // 0
  max?: number;  // 1
  step?: number;
  sensitive?: boolean;
  className?: ClassName;
  onChange?: (value: number) => void;
}
```

```tsx
<Scale value={volume} min={0} max={1} step={0.05} onChange={setVolume} />
```

### ToggleButton and CheckButton

A button that stays pressed, [togglebutton](/applets/exec/reference/elements#togglebutton), and a check box, [checkbutton](/applets/exec/reference/elements#checkbutton).

```ts
interface ToggleButtonProps {
  label?: string;
  icon?: string;
  active?: boolean;
  className?: ClassName;
  onToggle?: (active: boolean) => void;
}

interface CheckButtonProps {
  label?: string;
  active?: boolean;
  className?: ClassName;
  onToggle?: (active: boolean) => void;
}
```

```tsx
<Box spacing={6}>
  <ToggleButton icon="starred-symbolic" label="Starred" active={focused} onToggle={setFocused} />
  <CheckButton label="Show done" active={done} onToggle={setDone} />
</Box>
```

### DropDown

A compact choice from a list of strings. See [dropdown](/applets/exec/reference/elements#dropdown).

```ts
interface DropDownProps {
  options?: string[];
  selected?: number;
  className?: ClassName;
  onSelect?: (index: number) => void;
}
```

```tsx
<DropDown options={["Today", "This week", "All"]} selected={range} onSelect={setRange} />
```

### ChoiceList

A visible list of choices. See [choicelist](/applets/exec/reference/elements#choicelist).

```ts
interface ChoiceListProps {
  choices?: { label: string; detail?: string; icon?: string; warning?: boolean }[];
  selected?: number;
  className?: ClassName;
  onSelect?: (index: number) => void;
}
```

```tsx
<ChoiceList
  choices={[
    { label: "Balanced", icon: "power-profile-balanced-symbolic" },
    { label: "Power saver", detail: "Slower, quieter", icon: "power-profile-power-saver-symbolic" },
  ]}
  selected={range}
  onSelect={setRange}
/>
```

### Calendar

A month grid. See [calendar](/applets/exec/reference/elements#calendar).

```ts
interface CalendarProps {
  year?: number;
  month?: number;
  selected?: string;
  today?: string;
  events?: { date: string; colors?: Tone[] }[];
  className?: ClassName;
  onMonth?: (year: number, month: number) => void;
  onSelect?: (date: string) => void;
}
```

```tsx
<Calendar
  year={2026}
  month={10}
  today="2026-10-18"
  events={[{ date: "2026-10-21", colors: ["accent"] }]}
  onSelect={(date) => setDraft(date)}
/>
```

### Notice

A highlighted message. See [notice](/applets/exec/reference/elements#notice).

```ts
interface NoticeProps {
  icon?: string;
  title?: string;
  subtitle?: string;
  severity?: "info" | "warning" | "error";
  className?: ClassName;
  onActivate?: () => void;
}
```

```tsx
<Notice icon="dialog-warning-symbolic" title="Sync failed" subtitle="Click to retry" severity="warning" onActivate={() => setDone(false)} />
```

## Layout and display

### Box

Lays out body elements in a row or a column. See [box](/applets/exec/reference/elements#box).

```ts
interface BoxProps {
  orientation?: "horizontal" | "vertical"; // "horizontal" when absent
  spacing?: number;
  homogeneous?: boolean;
  halign?: Align;
  valign?: Align;
  hexpand?: boolean;
  vexpand?: boolean;
  className?: ClassName;
  children?: Node;
}
```

```tsx
<Box orientation="vertical" spacing={4}>
  <Label text="Next up" className="heading" />
  <Label text="Buy groceries" className="dim-label" />
</Box>
```

### Label

Plain text. See [label](/applets/exec/reference/elements#label).

```ts
interface LabelProps {
  text?: string;
  wrap?: boolean;
  xalign?: number;
  ellipsize?: "none" | "start" | "middle" | "end";
  lines?: number;
  className?: ClassName;
  children?: Node; // text children become the text
}
```

```tsx
<Label text="Pick up the parcel before the post office closes at six." wrap lines={2} ellipsize="end" />
```

### Image

A themed icon. See [image](/applets/exec/reference/elements#image).

```ts
interface ImageProps {
  icon?: string;
  tooltip?: string;
  pixelSize?: number;
  className?: ClassName;
}
```

```tsx
<Image icon="weather-clear-symbolic" pixelSize={32} tooltip="Clear sky" />
```

### Swatch

A chip filled with a CSS color. See [swatch](/applets/exec/reference/elements#swatch).

```ts
interface SwatchProps {
  color?: string;
  tooltip?: string;
  size?: "regular" | "large";
}
```

```tsx
<Popover title="Picked color">
  <Addon>
    <Swatch color="#e0563f" size="large" tooltip="#e0563f" />
  </Addon>
</Popover>
```

### Placeholder

An empty state. See [placeholder](/applets/exec/reference/elements#placeholder).

```ts
interface PlaceholderProps {
  icon?: string;
  title?: string;
  description?: string;
  className?: ClassName;
}
```

```tsx
<Placeholder icon="checkbox-checked-symbolic" title="All done" description="Nothing left for today." />
```

### Spinner and Separator

A busy spinner, [spinner](/applets/exec/reference/elements#spinner), and a line between groups, [separator](/applets/exec/reference/elements#separator).

```ts
interface SpinnerProps {
  className?: ClassName;
}

interface SeparatorProps {
  orientation?: "horizontal" | "vertical"; // "horizontal" when absent
  className?: ClassName;
}
```

```tsx
<Box orientation="vertical">
  <Spinner />
  <Separator />
</Box>
```

### FactList

Label and value pairs. See [factlist](/applets/exec/reference/elements#factlist).

```ts
interface FactListProps {
  facts?: { label: string; value: string; warning?: boolean }[];
}
```

```tsx
<FactList
  facts={[
    { label: "Due", value: "Friday" },
    { label: "Overdue", value: "2 tasks", warning: true },
  ]}
/>
```

### Readout

A large value with a unit. See [readout](/applets/exec/reference/elements#readout).

```ts
interface ReadoutProps {
  value?: string;
  unit?: string;
  className?: ClassName;
}
```

```tsx
<Readout value="21" unit="°C" />
```

### Dots

A row of small tone dots. See [dots](/applets/exec/reference/elements#dots).

```ts
interface DotsProps {
  colors?: Tone[];
  uniform?: boolean;
  size?: number;
  max?: number;
  className?: ClassName;
}
```

```tsx
<Dots colors={["success", "success", "warning"]} />
```

### Revealer and Stack

Show or hide children, [revealer](/applets/exec/reference/elements#revealer), and show one child by index, [stack](/applets/exec/reference/elements#stack).

```ts
interface RevealerProps {
  revealed?: boolean;
  transition?: "slide-down" | "crossfade" | "none"; // "slide-down" when absent
  className?: ClassName;
  children?: Node;
}

interface StackProps {
  visible?: number; // 0 when absent
  className?: ClassName;
  children?: Node;
}
```

```tsx
<Stack visible={done ? 1 : 0}>
  <Label text="Working on it" />
  <Label text="Done" />
</Stack>
```

### RangeBar

A range on a scale. See [rangebar](/applets/exec/reference/elements#rangebar).

```ts
interface RangeBarProps {
  min?: number;  // 0
  max?: number;  // 1
  low?: number;
  high?: number;
  now?: number;
  className?: ClassName;
}
```

```tsx
<RangeBar min={-5} max={30} low={8} high={19} now={14} />
```

### Bars and Sparkline

Small charts, [bars](/applets/exec/reference/elements#bars) and [sparkline](/applets/exec/reference/elements#sparkline).

```ts
interface BarsProps {
  values?: Array<number | null>;
  maximum?: number;
  highlightLast?: boolean; // true when absent
  label?: string;
  tooltips?: Array<string | null>;
  className?: ClassName;
}

interface SparklineProps {
  values?: Array<number | null>;
  fill?: boolean;
  showLast?: boolean;
  label?: string;
  className?: ClassName;
}
```

```tsx
<Box orientation="vertical" spacing={6}>
  <Bars values={[3, 5, null, 2, 4]} tooltips={["Mon", "Tue", null, "Thu", "Fri"]} label="Tasks done per day" />
  <Sparkline values={[18, 19, 21, 20, 22]} showLast label="Temperature today" />
</Box>
```

## Hooks

```ts
function useState<T>(initial: T | (() => T)): [T, (action: T | ((prev: T) => T)) => void];
function useEffect(effect: () => void | (() => void), deps?: readonly unknown[]): void;
function useOptions<T extends Record<string, unknown> = Record<string, unknown>>(): T;
function usePlacement(): Placement | null;
function usePopover(): { open: boolean; close: () => void };
function useAppletName(): string;
function useStateDir(): string;
function usePersistentState<T>(key: string, initial: T): [T, (action: T | ((prev: T) => T)) => void];
```

- `useState` and `useEffect` are React's.
- `useOptions` returns the current options, which update without a restart. The type parameter is a cast, not validation.
- `usePlacement` returns `null` until the panel sends the placement.
- `usePopover` tells whether the popover is open, and `close` closes it. `close` is the same function on every render.
- `useAppletName` returns this configured instance's name.
- `useStateDir` returns the applet's own directory for files, created with mode `0700`.
- `usePersistentState` works like `useState` and saves `key` to `state.json` in that directory. It loads the saved value on the first render; a missing or unreadable file gives `initial`. Several keys share one file.

## Desktop actions

```ts
function notify(message: {
  summary: string;
  body?: string;
  icon?: string;
  urgency?: "low" | "normal" | "critical";
}): void;
function copy(text: string): void;
function openUri(uri: string): void;
function launch(id: string): void;
```

`notify` defaults to an empty body and normal urgency. It runs `notify-send` from the applet process and `copy` runs `wl-copy`, so libnotify and wl-clipboard must be installed; the notification daemon applies its own limits. `launch` takes an installed desktop-file id with or without `.desktop`. `openUri` accepts only `http`, `https`, `mailto`, and eligible `file` URIs, at most 2,048 bytes. See [Notify, copy, open links and launch apps](/applets/exec/host) for file restrictions.

## Runtime and JSX exports

```ts
function run(element: unknown, io?: Io): Promise<void>;
function createElement(
  type: unknown,
  props?: Record<string, unknown> | null,
  ...children: unknown[]
): unknown;
const Fragment: typeof React.Fragment;
function jsx(type: unknown, props: unknown, key?: unknown): unknown;
function jsxs(type: unknown, props: unknown, key?: unknown): unknown;
function jsxDEV(type: unknown, props: unknown, key?: unknown): unknown;

interface Io {
  writeLine: (line: string) => void;
  lines: AsyncIterable<string | string[]>;
}

interface Placement {
  output: string | null;
  position: string;
  orientation: string;
  zone: string;
  size: number;
}

type Node =
  | string | number | boolean | null | undefined | Node[]
  | { type: unknown; props: unknown; key: unknown };

type FC<P = object> = (props: P & { key?: string | number }) => Node;
```

`run` connects to the socket named by `GLIMPSE_APPLET_SOCKET`. Without that variable and without an
`io`, it prints `GLIMPSE_APPLET_SOCKET is not set: run the applet under the panel` and exits 1. A
supplied `Io` replaces the socket. `run` redirects console methods to stderr and exits 0 when the
session ends. `jsx`, `jsxs` and `jsxDEV` are what the JSX runtime calls; you never call them.

The host's placement values are `top`, `bottom`, `left`, or `right` for `position`; `horizontal` or `vertical` for `orientation`; and `left`, `center`, or `right` for `zone`. `size` is the bar's logical-pixel thickness, and `output` is its connector name or `null`.

The separate `glimpse-sdk/testing` entry point exports `render` and the `Rendered` type. See [Test an applet](/applets/exec/testing) for its methods.
