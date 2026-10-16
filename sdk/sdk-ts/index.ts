import {
  type Io,
  nameStore,
  openStore,
  optionsStore,
  type Placement,
  placementStore,
  redirectConsole,
  send,
  servicesStore,
  session,
  stateStore,
  socketIo,
  standardIo,
} from "./reconciler.ts";
import * as react from "react";

redirectConsole();

export { Fragment, jsx, jsxDEV, jsxs } from "./jsx-runtime.ts";

export type Node =
  | string
  | number
  | boolean
  | null
  | undefined
  | Node[]
  | { type: unknown; props: unknown; key: unknown };

export type FC<P = object> = (props: P & { key?: string | number }) => Node;

export type ClassName =
  | "dim-label"
  | "caption"
  | "heading"
  | "title-1"
  | "title-2"
  | "title-3"
  | "title-4"
  | "numeric"
  | "accent"
  | "success"
  | "warning"
  | "error"
  | "flat"
  | "pill"
  | "circular"
  | "column"
  | "network-popover__rule"
  | "bluetooth-popover"
  | "network-popover";

export type Tone = "accent" | "dim" | "success" | "warning" | "error";

export type Align = "fill" | "start" | "end" | "center" | "baseline";

export type PopoverSize = "regular" | "wide";

export type Paint =
  | "red"
  | "orange"
  | "yellow"
  | "green"
  | "blue"
  | "purple"
  | "light"
  | "dark";

export const Indicator = "indicator" as unknown as FC<{
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
  children?: Node;
}>;

export const Popover = "popover" as unknown as FC<{
  className?: ClassName;
  icon?: string;
  title?: string;
  subtitle?: string;
  size?: PopoverSize;
  children?: Node;
}>;

export const Addon = "addon" as unknown as FC<{ children?: Node }>;

export const Container = "container" as unknown as FC<{
  width?: number;
  height?: number;
  background?: Paint;
  color?: Paint;
  align?: Align;
  children?: Node;
}>;

export const Section = "section" as unknown as FC<{
  title?: string;
  count?: string;
  className?: ClassName;
  children?: Node;
}>;

export type RowTone = "warning" | "error" | "destructive";
export type RowTypography = "numeric" | "monospace";
type RowProps = {
  icon?: string;
  title?: string;
  subtitle?: string;
  value?: string;
  tone?: RowTone;
  inactive?: boolean;
  tooltip?: string;
  typography?: RowTypography;
  busy?: boolean;
};

export const Row = "row" as unknown as FC<
  Omit<RowProps, "busy"> & { children?: Node }
>;
export const ActionRow = "actionrow" as unknown as FC<
  RowProps & { onActivate?: () => void; children?: Node }
>;
export const ToggleRow = "togglerow" as unknown as FC<
  RowProps & {
    control?: "check" | "switch";
    active?: boolean;
    onToggle?: (active: boolean) => void;
  }
>;
export const MeterRow = "meterrow" as unknown as FC<
  RowProps & { level?: number; children?: Node }
>;
export const ExpanderRow = "expanderrow" as unknown as FC<
  RowProps & {
    open?: boolean;
    tint?: boolean;
    titleLines?: number;
    onToggle?: (open: boolean) => void;
    children?: Node;
  }
>;
export const End = "end" as unknown as FC<{ children?: Node }>;
export const Start = "start" as unknown as FC<{ children?: Node }>;
export const SliderRow = "sliderrow" as unknown as FC<{
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
}>;

export const FactList = "factlist" as unknown as FC<{
  facts?: { label: string; value: string; warning?: boolean }[];
}>;

export const ChoiceList = "choicelist" as unknown as FC<{
  choices?: {
    label: string;
    detail?: string;
    icon?: string;
    warning?: boolean;
  }[];
  selected?: number;
  className?: ClassName;
  onSelect?: (index: number) => void;
}>;

export const Readout = "readout" as unknown as FC<{
  value?: string;
  unit?: string;
  className?: ClassName;
}>;

export const Notice = "notice" as unknown as FC<{
  icon?: string;
  title?: string;
  subtitle?: string;
  severity?: "info" | "warning" | "error";
  className?: ClassName;
  onActivate?: () => void;
}>;

export const Calendar = "calendar" as unknown as FC<{
  year?: number;
  month?: number;
  selected?: string;
  today?: string;
  events?: { date: string; colors?: Tone[] }[];
  className?: ClassName;
  onMonth?: (year: number, month: number) => void;
  onSelect?: (date: string) => void;
}>;

export const Dots = "dots" as unknown as FC<{
  colors?: Tone[];
  uniform?: boolean;
  size?: number;
  max?: number;
  className?: ClassName;
}>;

export const DropDown = "dropdown" as unknown as FC<{
  options?: string[];
  selected?: number;
  className?: ClassName;
  onSelect?: (index: number) => void;
}>;

export const ToggleButton = "togglebutton" as unknown as FC<{
  label?: string;
  icon?: string;
  active?: boolean;
  className?: ClassName;
  onToggle?: (active: boolean) => void;
}>;

export const CheckButton = "checkbutton" as unknown as FC<{
  label?: string;
  active?: boolean;
  className?: ClassName;
  onToggle?: (active: boolean) => void;
}>;

export const Revealer = "revealer" as unknown as FC<{
  revealed?: boolean;
  transition?: "slide-down" | "crossfade" | "none";
  className?: ClassName;
  children?: Node;
}>;

export const Stack = "stack" as unknown as FC<{
  visible?: number;
  className?: ClassName;
  children?: Node;
}>;

export const RangeBar = "rangebar" as unknown as FC<{
  min?: number;
  max?: number;
  low?: number;
  high?: number;
  now?: number;
  className?: ClassName;
}>;

export const Bars = "bars" as unknown as FC<{
  values?: Array<number | null>;
  maximum?: number;
  highlightLast?: boolean;
  label?: string;
  tooltips?: Array<string | null>;
  className?: ClassName;
}>;

export const Sparkline = "sparkline" as unknown as FC<{
  values?: Array<number | null>;
  fill?: boolean;
  showLast?: boolean;
  label?: string;
  className?: ClassName;
}>;

export const Entry = "entry" as unknown as FC<{
  placeholder?: string;
  value?: string;
  className?: ClassName;
  onChange?: (value: string) => void;
  onSubmit?: (value: string) => void;
}>;

export const Placeholder = "placeholder" as unknown as FC<{
  icon?: string;
  title?: string;
  description?: string;
  className?: ClassName;
}>;

export const Box = "box" as unknown as FC<{
  orientation?: "horizontal" | "vertical";
  spacing?: number;
  homogeneous?: boolean;
  halign?: Align;
  valign?: Align;
  hexpand?: boolean;
  vexpand?: boolean;
  className?: ClassName;
  children?: Node;
}>;

export const Label = "label" as unknown as FC<{
  text?: string;
  wrap?: boolean;
  xalign?: number;
  ellipsize?: "none" | "start" | "middle" | "end";
  lines?: number;
  className?: ClassName;
  children?: Node;
}>;

export const Image = "image" as unknown as FC<{
  icon?: string;
  tooltip?: string;
  pixelSize?: number;
  className?: ClassName;
}>;

export const Swatch = "swatch" as unknown as FC<{
  color?: string;
  tooltip?: string;
  size?: "regular" | "large";
}>;

export const Button = "button" as unknown as FC<{
  text?: string;
  icon?: string;
  tooltip?: string;
  sensitive?: boolean;
  className?: ClassName;
  onClick?: () => void;
  children?: Node;
}>;

export const Switch = "switch" as unknown as FC<{
  active?: boolean;
  sensitive?: boolean;
  className?: ClassName;
  onToggle?: (active: boolean) => void;
}>;

export const Scale = "scale" as unknown as FC<{
  value?: number;
  min?: number;
  max?: number;
  step?: number;
  sensitive?: boolean;
  className?: ClassName;
  onChange?: (value: number) => void;
}>;

export const Spinner = "spinner" as unknown as FC<{
  className?: ClassName;
}>;

export const Separator = "separator" as unknown as FC<{
  orientation?: "horizontal" | "vertical";
  className?: ClassName;
}>;

export type { Io, Placement };

export function useState<T>(
  initial: T | (() => T),
): [T, (action: T | ((prev: T) => T)) => void] {
  return react.useState(initial);
}

export function useEffect(
  effect: () => void | (() => void),
  deps?: readonly unknown[],
): void {
  react.useEffect(effect, deps);
}

export function useOptions<
  T extends Record<string, unknown> = Record<string, unknown>,
>(): T {
  return react.useSyncExternalStore(
    optionsStore.subscribe,
    optionsStore.get,
  ) as T;
}

export function usePlacement(): Placement | null {
  return react.useSyncExternalStore(
    placementStore.subscribe,
    placementStore.get,
  );
}

export function usePopoverOpen(): boolean {
  return react.useSyncExternalStore(openStore.subscribe, openStore.get);
}

export function useAppletName(): string {
  return react.useSyncExternalStore(nameStore.subscribe, nameStore.get);
}

export function useStateDir(): string {
  return react.useSyncExternalStore(stateStore.subscribe, stateStore.get);
}

export function createElement(
  type: unknown,
  props?: Record<string, unknown> | null,
  ...children: unknown[]
): unknown {
  return react.createElement(
    type as react.ElementType,
    props ?? null,
    ...children as react.ReactNode[],
  );
}

export function notify(message: {
  summary: string;
  body?: string;
  icon?: string;
  urgency?: "low" | "normal" | "critical";
}): void {
  const wire: Record<string, unknown> = {
    t: "notify",
    summary: message.summary,
  };
  if (message.body !== undefined) wire.body = message.body;
  if (message.icon !== undefined) wire.icon = message.icon;
  if (message.urgency !== undefined) wire.urgency = message.urgency;
  send(wire);
}

export function copy(text: string): void {
  send({ t: "copy", text });
}

export function openUri(uri: string): void {
  send({ t: "open-uri", uri });
}

export type Player = {
  id: string;
  identity: string;
  desktopEntry: string | null;
  playback: "playing" | "paused" | "stopped";
  current: boolean;
  title: string | null;
  artist: string | null;
  album: string | null;
  lengthUs: number | null;
  positionUs: number;
  positionAt: number;
  rate: number;
  can: {
    play: boolean;
    pause: boolean;
    previous: boolean;
    next: boolean;
    seek: boolean;
  };
};

export type Workspace = {
  id: number;
  index: number | null;
  name: string | null;
  output: string | null;
  active: boolean;
  focused: boolean;
  urgent: boolean;
  windows: number;
};

export type Window = {
  id: number;
  title: string | null;
  appId: string | null;
  workspace: number | null;
  focused: boolean;
  urgent: boolean;
};

export type Output = {
  connector: string;
  label: string | null;
  focused: boolean;
};

export type CompositorSnapshot = {
  name: string | null;
  workspaces: Workspace[];
  windows: Window[];
  outputs: Output[];
};

export type PlayerAction =
  | "play"
  | "pause"
  | "play-pause"
  | "stop"
  | "previous"
  | "next"
  | "raise";

export type Step = "next" | "prev";

export type WorkspaceTarget =
  | { id: number }
  | { index: number }
  | { name: string }
  | Step;

export type WindowTarget = { id: number } | Step;

function useService<T>(name: string): T | null {
  return react.useSyncExternalStore(
    servicesStore.subscribe,
    () => (servicesStore.get()[name] as T | undefined) ?? null,
  );
}

export function useMpris(): Player[] | null {
  return useService<{ players: Player[] }>("mpris")?.players ?? null;
}

export function useCompositor(): CompositorSnapshot | null {
  return useService<CompositorSnapshot>("compositor");
}

export function positionNow(player: Player, now: number = Date.now()): number {
  if (player.playback !== "playing") return player.positionUs;
  const position = player.positionUs +
    (now - player.positionAt) * 1000 * player.rate;
  return player.lengthUs ? Math.min(position, player.lengthUs) : position;
}

export const mpris = {
  control(player: string, action: PlayerAction): void {
    send({ t: "mpris", player, op: "control", action });
  },
  seek(player: string, offsetUs: number): void {
    send({ t: "mpris", player, op: "seek", offset: Math.round(offsetUs) });
  },
  setPosition(player: string, positionUs: number): void {
    send({
      t: "mpris",
      player,
      op: "set-position",
      position: Math.round(positionUs),
    });
  },
};

export const compositor = {
  focusWorkspace(target: WorkspaceTarget): void {
    send({ t: "compositor", op: "focus-workspace", target });
  },
  focusWindow(target: WindowTarget): void {
    send({ t: "compositor", op: "focus-window", target });
  },
  moveWindow(window: number, target: WorkspaceTarget): void {
    send({ t: "compositor", op: "move-window", window, target });
  },
  renameWorkspace(id: number, name: string | null): void {
    send({ t: "compositor", op: "rename-workspace", id, name });
  },
  closeWindow(id: number): void {
    send({ t: "compositor", op: "close-window", id });
  },
};

export function launch(id: string): void {
  send({ t: "launch", id });
}

export function closePopover(): void {
  send({ t: "close-popover" });
}

function defaultIo(): Io {
  const path = process.env.GLIMPSE_APPLET_SOCKET;
  return path ? socketIo(path) : standardIo();
}

export async function run(element: unknown, io?: Io): Promise<void> {
  redirectConsole();
  await session(element, io ?? defaultIo());
  process.exit(0);
}
