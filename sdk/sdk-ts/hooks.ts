import fs from "node:fs";
import path from "node:path";
import * as react from "react";

import {
	nameStore,
	openStore,
	optionsStore,
	placementStore,
	stateStore,
} from "./state.ts";
import type { Placement } from "./wire.ts";

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

function readStateFile(dir: string): Record<string, unknown> {
	if (!dir) return {};
	try {
		const parsed: unknown = JSON.parse(
			fs.readFileSync(path.join(dir, "state.json"), "utf8"),
		);
		if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
			return parsed as Record<string, unknown>;
		}
	} catch {
		return {};
	}
	return {};
}

function writeStateFile(dir: string, key: string, value: unknown): void {
	if (!dir) return;
	const all = readStateFile(dir);
	all[key] = value;
	try {
		fs.writeFileSync(path.join(dir, "state.json"), JSON.stringify(all));
	} catch {
		return;
	}
}

export function usePersistentState<T>(
	key: string,
	initial: T,
): [T, (action: T | ((prev: T) => T)) => void] {
	const dir = useStateDir();
	const [value, setValue] = react.useState<T>(() => {
		const saved = readStateFile(dir);
		return key in saved ? (saved[key] as T) : initial;
	});
	const current = react.useRef(value);
	current.current = value;
	const set = react.useCallback(
		(action: T | ((prev: T) => T)) => {
			const next = typeof action === "function"
				? (action as (prev: T) => T)(current.current)
				: action;
			if (Object.is(next, current.current)) return;
			current.current = next;
			writeStateFile(stateStore.get(), key, next);
			setValue(next);
		},
		[key],
	);
	return [value, set];
}
