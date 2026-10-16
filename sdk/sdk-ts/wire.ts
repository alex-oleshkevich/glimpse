export type Placement = {
	output: string | null;
	position: string;
	orientation: string;
	zone: string;
	size: number;
};

export type Io = {
	writeLine: (line: string) => void;
	lines: AsyncIterable<string | string[]>;
};

export type Json =
	| string
	| number
	| boolean
	| null
	| Json[]
	| { [key: string]: Json };
export type Op = { [key: string]: Json };
export type Props = Record<string, unknown>;
export type Handler = (...args: unknown[]) => void;

export type HostNode = {
	id: number;
	type: string;
	props: Props;
	children: HostNode[];
	attached: boolean;
	handlers: Map<string, Handler>;
	appliedSeq?: number;
	confirmSeq?: number;
};

export type Reconciler = {
	createContainer: (...args: unknown[]) => unknown;
	updateContainer: (...args: unknown[]) => void;
	flushSyncFromReconciler: (fn?: () => void) => void;
	flushPassiveEffects?: () => boolean;
};

export type Session = {
	nextId: number;
	nodes: Map<number, HostNode>;
	ops: Op[];
	pending: Op[];
	container: HostNode;
	reconciler: Reconciler;
	root: unknown;
	flushing: boolean;
	scheduled: boolean;
};

export type HostMessage =
	| {
			t: "hello";
			v?: number;
			name?: string;
			state?: string;
			options?: Props;
			placement?: Placement;
	  }
	| { t: "options"; options?: Props }
	| { t: "placement"; placement?: Placement }
	| { t: "popover"; open?: boolean }
	| { t: "service"; name?: string; value?: unknown }
	| {
			t: "event";
			id?: number;
			name?: string;
			args?: unknown[];
			seq?: number | null;
	  };
export const TEXT = new Set(["label", "indicator", "button"]);
export const SKIP = new Set(["children", "key", "ref"]);
export const encoder = new TextEncoder();
export class ProtocolLimitError extends Error {
	override name = "ProtocolLimitError";
}
export function lineOf(message: unknown): string {
	const line = JSON.stringify(message);
	if (encoder.encode(line).byteLength > 1_048_576) {
		throw new ProtocolLimitError("outgoing line exceeds 1 MiB");
	}
	return line;
}
export function textOf(children: unknown): string | undefined {
	const values = [children]
		.flat(Infinity)
		.filter(
			(child) =>
				child !== null && child !== undefined && typeof child !== "boolean",
		);
	return values.length &&
		values.every(
			(child) => typeof child === "string" || typeof child === "number",
		)
		? values.join("")
		: undefined;
}

export function capped(value: string, limit: number): string {
	return [...value].slice(0, limit).join("");
}

export function wireProps(type: string, props: Props): Props {
	const out: Props = {};
	const text = TEXT.has(type) ? textOf(props.children) : undefined;
	for (const key of Object.keys(props)) {
		if (SKIP.has(key)) continue;
		if (key === "text") {
			if (text !== undefined) out.text = capped(text, 256);
			else if (
				typeof props.text === "string" ||
				typeof props.text === "number"
			) {
				out.text = capped(String(props.text), 256);
			}
			continue;
		}
		if (!Object.hasOwn(props, key)) continue;
		const value = props[key];
		if (typeof value === "function") {
			out[key] = true;
			continue;
		}
		if (value === undefined) continue;
		out[key] =
			typeof value === "string"
				? capped(
						value,
						key === "badge"
							? 8
							: type === "entry" && key === "value"
								? 4096
								: 256,
					)
				: value;
	}
	if (text !== undefined && !Object.hasOwn(out, "text")) {
		out.text = capped(text, 256);
	}
	return out;
}

export function same(left: unknown, right: unknown): boolean {
	if (Object.is(left, right)) return true;
	if (left === null || right === null) return false;
	if (typeof left !== "object" || typeof right !== "object") return false;
	return JSON.stringify(left) === JSON.stringify(right);
}

export function diffProps(
	type: string,
	prev: Props,
	next: Props,
): Props | null {
	const before = wireProps(type, prev);
	const after = wireProps(type, next);
	const seen = new Set<string>();
	const out: Props = {};
	let changed = false;
	for (const key of [...Object.keys(after), ...Object.keys(before)]) {
		if (seen.has(key)) continue;
		seen.add(key);
		const hasBefore = Object.hasOwn(before, key);
		const hasAfter = Object.hasOwn(after, key);
		if (!hasBefore && !hasAfter) continue;
		if (hasBefore && hasAfter && same(before[key], after[key])) continue;
		out[key] = hasAfter ? after[key] : null;
		changed = true;
	}
	return changed ? out : null;
}
export const CONTROLLED: Record<string, [string, string]> = {
	entry: ["onChange", "value"],
	scale: ["onChange", "value"],
	switch: ["onToggle", "active"],
	togglerow: ["onToggle", "active"],
	togglebutton: ["onToggle", "active"],
	checkbutton: ["onToggle", "active"],
	expanderrow: ["onToggle", "open"],
	sliderrow: ["onChange", "value"],
	dropdown: ["onSelect", "selected"],
	choicelist: ["onSelect", "selected"],
};
export const SERIES_KEYS = ["values", "tooltips"];

export function oversized(value: unknown, limit = 64): boolean {
	if (Array.isArray(value)) {
		return value.length > limit || value.some((item) => oversized(item));
	}
	if (value !== null && typeof value === "object") {
		return Object.values(value).some((field) => oversized(field));
	}
	return false;
}

export function propsOversized(props: Record<string, unknown>): boolean {
	return Object.entries(props).some(([key, value]) =>
		oversized(value, SERIES_KEYS.includes(key) ? 366 : 64),
	);
}
