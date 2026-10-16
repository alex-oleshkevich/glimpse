import * as react from "react";

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

export function createElement(
	type: unknown,
	props?: Record<string, unknown> | null,
	...children: unknown[]
): unknown {
	return react.createElement(
		type as react.ElementType,
		props ?? null,
		...(children as react.ReactNode[]),
	);
}
