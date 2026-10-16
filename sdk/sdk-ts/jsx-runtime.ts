import {
	Fragment,
	jsx as reactJsx,
	jsxs as reactJsxs,
} from "react/jsx-runtime";
import type { Node } from "./elements.ts";

type JsxFn = (type: unknown, props: unknown, key?: unknown) => unknown;

export const jsx = reactJsx as JsxFn;
export const jsxs = reactJsxs as JsxFn;
export { Fragment };

export function jsxDEV(type: unknown, props: unknown, key?: unknown): unknown {
	return jsx(type, props, key);
}

export namespace JSX {
	export type Element = Node;
	export interface ElementChildrenAttribute {
		children: unknown;
	}
	export interface IntrinsicElements {
		[elemName: string]: Record<string, unknown>;
	}
}
