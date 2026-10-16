import { createConnection } from "node:net";
import { createInterface } from "node:readline";
import { PassThrough, type Readable } from "node:stream";
import Reconciler from "react-reconciler";

import { blankNode, flush, makeHost } from "./host.ts";
import {
	nameStore,
	openStore,
	optionsStore,
	placementStore,
	redirectConsole,
	runtime,
	servicesStore,
	setEmit,
	stateStore,
	writeAll,
} from "./state.ts";
import {
	CONTROLLED,
	type HostMessage,
	type Io,
	lineOf,
	ProtocolLimitError,
	type Reconciler as ReconcilerInstance,
	type Session,
} from "./wire.ts";

const reconcilerFactory = Reconciler as unknown as (
	config: object,
) => ReconcilerInstance;

async function loadReconciler(): Promise<
	(config: object) => ReconcilerInstance
> {
	return reconcilerFactory;
}

async function mountFresh(element: unknown): Promise<void> {
	const factory = await loadReconciler();
	const container = blankNode("root");
	container.id = 0;
	container.attached = true;
	const session: Session = {
		nextId: 1,
		nodes: new Map(),
		ops: [],
		pending: [],
		container,
		reconciler: null as unknown as ReconcilerInstance,
		root: null,
		flushing: false,
		scheduled: false,
	};
	session.reconciler = factory(makeHost(session));
	session.root = session.reconciler.createContainer(
		container,
		0,
		null,
		false,
		null,
		"applet",
		(error: unknown) => {
			console.error(error);
			runtime.fail(70);
		},
		(error: unknown) => {
			console.error(error);
		},
		(error: unknown) => {
			console.error(error);
		},
		null,
	);
	runtime.current = session;
	flush(
		session,
		() => {
			session.reconciler.updateContainer(element, session.root, null, null);
		},
		false,
	);
}

function dispatchEvent(message: HostMessage): void {
	if (message.t !== "event" || !runtime.current) return;
	const id = message.id;
	if (typeof id !== "number") return;
	const node = runtime.current.nodes.get(id);
	if (!node) return;
	if (typeof message.name !== "string") return;
	const handler = node.handlers.get(message.name);
	if (!handler) return;
	if (
		CONTROLLED[node.type]?.[0] === message.name &&
		typeof message.seq === "number" &&
		Number.isFinite(message.seq)
	) {
		if (node.appliedSeq !== undefined && message.seq < node.appliedSeq) return;
		node.appliedSeq = message.seq;
		node.confirmSeq = message.seq;
	}
	const args = Array.isArray(message.args) ? message.args : [];
	try {
		handler(...args);
	} catch (error) {
		console.error(error);
		runtime.fail(1);
	}
}

function apply(message: HostMessage): void {
	if (!runtime.current) return;
	if (message.t === "options") {
		optionsStore.set(message.options ?? {});
		return;
	}
	if (message.t === "placement") {
		placementStore.set(message.placement ?? null);
		return;
	}
	if (message.t === "popover") {
		openStore.set(message.open === true);
		return;
	}
	if (message.t === "service") {
		if (typeof message.name === "string") {
			servicesStore.set({
				...servicesStore.get(),
				[message.name]: message.value,
			});
		}
		return;
	}
	if (message.t === "event") dispatchEvent(message);
}

export async function session(
	element: unknown,
	io: Io,
	onFail: (code: number) => void = (code) => process.exit(code),
): Promise<void> {
	runtime.fail = onFail;
	redirectConsole();
	await loadReconciler();
	runtime.current = null;
	runtime.helloWritten = false;
	setEmit((message) => io.writeLine(lineOf(message)));
	io.writeLine(lineOf({ t: "hello", v: 1 }));
	runtime.helloWritten = true;
	for (const message of runtime.queued.splice(0)) runtime.emit(message);
	for await (const chunk of io.lines) {
		try {
			const buffered: HostMessage[] = [];
			for (const line of Array.isArray(chunk) ? chunk : [chunk]) {
				let message: HostMessage;
				try {
					message = JSON.parse(line) as HostMessage;
				} catch (error) {
					console.error(error);
					continue;
				}
				if (
					!message ||
					typeof message !== "object" ||
					typeof message.t !== "string"
				)
					continue;
				if (message.t === "hello") {
					if (runtime.current) continue;
					if (message.v !== 1) {
						console.error(`unsupported host protocol version: ${message.v}`);
						runtime.fail(1);
						return;
					}
					optionsStore.set(message.options ?? {});
					placementStore.set(message.placement ?? null);
					nameStore.set(typeof message.name === "string" ? message.name : "");
					stateStore.set(
						typeof message.state === "string" ? message.state : "",
					);
					openStore.set(false);
					servicesStore.set({});
					await mountFresh(element);
					continue;
				}
				if (runtime.current) buffered.push(message);
			}
			if (runtime.current) {
				flush(runtime.current, () => {
					for (const message of buffered) apply(message);
				});
			}
		} catch (error) {
			if (!(error instanceof ProtocolLimitError)) throw error;
			console.error(error);
			runtime.fail(70);
			return;
		}
	}
}

export async function* stdinLines(
	input: Readable = process.stdin,
): AsyncIterable<string[]> {
	const reader = createInterface({ input, crlfDelay: Infinity });
	const pending: string[] = [];
	const batches: string[][] = [];
	let wake: (() => void) | undefined;
	let closed = false;
	reader.on("line", (line) => {
		if (line.trim() === "") return;
		pending.push(line);
		if (pending.length === 1) {
			setImmediate(() => {
				batches.push(pending.splice(0));
				wake?.();
				wake = undefined;
			});
		}
	});
	reader.on("close", () => {
		closed = true;
		wake?.();
		wake = undefined;
	});
	try {
		while (!closed || pending.length || batches.length) {
			if (!batches.length)
				await new Promise<void>((resolve) => {
					wake = resolve;
				});
			while (batches.length) yield batches.shift()!;
		}
	} finally {
		reader.close();
	}
}

export function socketIo(path: string): Io {
	const socket = createConnection(path);
	const input = new PassThrough();
	socket.pipe(input);
	socket.on("error", (error: NodeJS.ErrnoException) => {
		const gone = ["ENOENT", "ECONNREFUSED", "EPIPE", "ECONNRESET"].includes(
			error.code ?? "",
		);
		if (!gone) console.error(error);
		input.end();
		process.exit(gone ? 0 : 1);
	});
	socket.on("close", () => input.end());
	return {
		writeLine(line: string) {
			if (!socket.destroyed) socket.write(`${line}\n`);
		},
		lines: stdinLines(input),
	};
}
