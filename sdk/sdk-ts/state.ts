import fs from "node:fs";
import { inspect } from "node:util";

import { encoder, lineOf, type Placement, type Session } from "./wire.ts";

export type Store<T> = {
	get: () => T;
	set: (value: T) => void;
	subscribe: (listener: () => void) => () => void;
};

function createStore<T>(initial: T): Store<T> {
	let value = initial;
	const listeners = new Set<() => void>();
	return {
		get: () => value,
		set(next: T) {
			if (Object.is(next, value)) return;
			value = next;
			for (const listener of [...listeners]) listener();
		},
		subscribe(listener: () => void) {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
	};
}

export const optionsStore = createStore<Record<string, unknown>>({});
export const placementStore = createStore<Placement | null>(null);
export const nameStore = createStore("");
export const stateStore = createStore("");
export const openStore = createStore(false);
export type AppletEntry = { id: string; name: string; icon: string | null };
export const entryStore = createStore<AppletEntry | null>(null);

export function writeAll(fd: number, text: string): void {
	const bytes = encoder.encode(text);
	const pause = new Int32Array(new SharedArrayBuffer(4));
	for (let offset = 0; offset < bytes.length; ) {
		try {
			const written = fs.writeSync(fd, bytes, offset, bytes.length - offset);
			if (written === 0) throw new Error("zero-byte write");
			offset += written;
		} catch (error) {
			if ((error as NodeJS.ErrnoException).code !== "EAGAIN") throw error;
			Atomics.wait(pause, 0, 0, 2);
		}
	}
}

export const runtime: {
	current: Session | null;
	fail: (code: number) => void;
	helloWritten: boolean;
	queued: unknown[];
	emit: (message: unknown) => void;
} = {
	current: null,
	fail: (code) => process.exit(code),
	helloWritten: false,
	queued: [],
	emit: (message) => writeAll(1, `${lineOf(message)}\n`),
};

export function setEmit(write: (message: unknown) => void): void {
	runtime.emit = write;
}

export function send(message: unknown): void {
	if (runtime.helloWritten) runtime.emit(message);
	else runtime.queued.push(message);
}

const consoleMethods = [
	"log",
	"info",
	"debug",
	"dir",
	"dirxml",
	"table",
	"count",
	"countReset",
	"time",
	"timeLog",
	"timeEnd",
	"group",
	"groupCollapsed",
	"groupEnd",
	"trace",
] as const;
const originalConsole = new Map<string, (...args: unknown[]) => void>();

function writeStderr(...args: unknown[]): void {
	const text = args
		.map((value) => {
			if (typeof value === "string") return value;
			return inspect(value);
		})
		.join(" ");
	writeAll(2, `${text}\n`);
}

export function redirectConsole(): void {
	const target = console as unknown as Record<
		string,
		(...args: unknown[]) => void
	>;
	if (originalConsole.size) return;
	for (const method of consoleMethods) {
		originalConsole.set(method, target[method]);
		target[method] = writeStderr;
	}
}

export function restoreConsole(): void {
	const target = console as unknown as Record<
		string,
		(...args: unknown[]) => void
	>;
	for (const [method, original] of originalConsole) target[method] = original;
	originalConsole.clear();
}
