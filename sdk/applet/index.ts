import {
  type Io,
  nameStore,
  openStore,
  optionsStore,
  type Placement,
  placementStore,
  redirectConsole,
  send,
  session,
  stateStore,
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
  | "circular";

export type Tone = "accent" | "dim" | "success" | "warning" | "error";

type Align = "fill" | "start" | "end" | "center" | "baseline";

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
  children?: Node;
}>;

export const Hero = "hero" as unknown as FC<{
  icon?: string;
  title?: string;
  subtitle?: string;
  className?: ClassName;
  children?: Node;
}>;

export const Section = "section" as unknown as FC<{
  title?: string;
  count?: string;
  className?: ClassName;
  children?: Node;
}>;

export const Row = "row" as unknown as FC<{
  icon?: string;
  title?: string;
  subtitle?: string;
  value?: string;
  selected?: boolean;
  busy?: boolean;
  chevron?: boolean;
  className?: ClassName;
  onActivate?: () => void;
  children?: Node;
}>;

export const SwitchRow = "switchrow" as unknown as FC<{
  icon?: string;
  title?: string;
  subtitle?: string;
  active?: boolean;
  busy?: boolean;
  className?: ClassName;
  onToggle?: (active: boolean) => void;
}>;

export const Fader = "fader" as unknown as FC<{
  icon?: string;
  value?: number;
  maximum?: number;
  floor?: number;
  muted?: boolean;
  className?: ClassName;
  onChange?: (value: number) => void;
  onMute?: () => void;
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

export const Expander = "expander" as unknown as FC<{
  title?: string;
  subtitle?: string;
  expanded?: boolean;
  className?: ClassName;
  onToggle?: (expanded: boolean) => void;
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

export const Footer = "footer" as unknown as FC<{
  className?: ClassName;
  children?: Node;
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

export const Progress = "progress" as unknown as FC<{
  text?: string;
  fraction?: number;
  className?: ClassName;
  children?: Node;
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

export type SessionAction =
  | "lock"
  | "suspend"
  | "hibernate"
  | "log-out"
  | "reboot"
  | "power-off";

export function sessionAction(action: SessionAction): void {
  send({ t: "session", action });
}

export { sessionAction as session };

export function closePopover(): void {
  send({ t: "close-popover" });
}

export async function run(element: unknown, io?: Io): Promise<void> {
  redirectConsole();
  await session(element, io ?? standardIo());
  process.exit(0);
}
