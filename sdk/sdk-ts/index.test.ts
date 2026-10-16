import fs from "node:fs";
import { spawn } from "node:child_process";
import { once } from "node:events";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { PassThrough } from "node:stream";
import test from "node:test";
import { setTimeout as delay } from "node:timers/promises";
import {
	openStore,
	optionsStore,
	placementStore,
	redirectConsole,
	restoreConsole,
	session,
	setEmit,
	socketIo,
	stdinLines,
} from "./reconciler.ts";
import {
	ActionRow,
	Box,
	Button,
	Calendar,
	CheckButton,
	ChoiceList,
	closePopover,
	copy,
	createElement,
	Dots,
	DropDown,
	Entry,
	End,
	Start,
	ExpanderRow,
	FactList,
	Fragment,
	Image,
	Indicator,
	Label,
	MeterRow,
	notify,
	Notice,
	openUri,
	Popover,
	RangeBar,
	Bars,
	Sparkline,
	Readout,
	Revealer,
	Row,
	run,
	Section,
	SliderRow,
	Stack,
	useOptions,
	useStateDir,
	ToggleButton,
	ToggleRow,
	useEffect,
	useState,
} from "./index.ts";

function assert(condition: unknown, message: string): asserts condition {
	if (!condition) throw new Error(message);
}

function fixture(name: string): URL {
	return new URL(`./fixtures/${name}`, import.meta.url);
}

function canonical(value: unknown): unknown {
	if (Array.isArray(value)) return value.map(canonical);
	if (value && typeof value === "object") {
		return Object.fromEntries(
			Object.entries(value)
				.sort(([a], [b]) => a.localeCompare(b))
				.map(([key, entry]) => [key, canonical(entry)]),
		);
	}
	return value;
}

function assertMessages(actual: string[], path: URL): void {
	const expected = fs
		.readFileSync(path, "utf8")
		.trim()
		.split("\n")
		.map((line) => JSON.parse(line));
	const got = actual.map((line) => JSON.parse(line));
	assert(
		JSON.stringify(canonical(got)) === JSON.stringify(canonical(expected)),
		`${path.pathname}\nexpected: ${JSON.stringify(expected)}\nactual: ${JSON.stringify(
			got,
		)}`,
	);
}

async function drive(element: unknown, messages: unknown[]): Promise<string[]> {
	const lines: string[] = [];
	await session(element, {
		writeLine: (line) => lines.push(line),
		lines: (async function* () {
			for (const message of messages) {
				if (Array.isArray(message)) {
					yield message.map((item) =>
						typeof item === "string" ? item : JSON.stringify(item),
					);
				} else {
					yield typeof message === "string" ? message : JSON.stringify(message);
				}
			}
		})(),
	});
	return lines;
}

function hostLine(): string {
	const text = fs.readFileSync(fixture("host.ndjson"), "utf8");
	const line = text.split("\n")[0];
	assert(line, "host.ndjson has a hello");
	return line;
}

function Ticking() {
	const [text, setText] = useState("before");
	useEffect(() => {
		const timer = setTimeout(() => setText("after"), 5);
		return () => clearTimeout(timer);
	}, []);
	return createElement(Label, { text });
}

function HelloApp() {
	return createElement(
		Fragment,
		null,
		createElement(
			Indicator,
			{ icon: "view-list-symbolic", tooltip: "Todos" },
			"2",
		),
		createElement(
			Popover,
			{ icon: "view-list-symbolic", title: "Todos", subtitle: "2 open" },
			createElement(
				Section,
				{ title: "Open", count: "2" },
				createElement(Row, { title: "Buy milk" }),
				createElement(ActionRow, {
					title: "Ship applets",
					onActivate: () => {},
				}),
			),
		),
	);
}

function rowProps(key: string, step: number): Record<string, unknown> {
	const props: Record<string, unknown> = { key };
	if (key === "a" && step === 7) props.title = "Sequenced";
	else if (key === "a" && step === 6) props.title = "Updated";
	else if (!(key === "a" && step >= 8)) props.title = key;
	if (key === "a" && step >= 6) props.busy = "nope";
	return props;
}

function OpsApp() {
	const options = useOptions<{ step?: number }>();
	const step = options.step ?? 0;
	const keys =
		step < 1
			? []
			: step < 2
				? ["a", "b", "c"]
				: step < 3
					? ["c", "a", "b"]
					: ["c", "a", "d", "b"];
	const children: unknown[] = [];
	if (step >= 1) {
		children.push(
			createElement(
				Section,
				{ title: "Open" },
				...keys.map((key) => createElement(Row, rowProps(key, step))),
			),
		);
	}
	if (step === 4) {
		children.push(
			createElement(
				Box,
				null,
				createElement(Label, null, "one"),
				createElement(Label, null, "two"),
			),
		);
	}
	return createElement(Popover, null, ...children);
}

function EntryApp() {
	const [value, setValue] = useState("x");
	return createElement(
		Popover,
		null,
		createElement(Entry, {
			value,
			onChange: (next: string) => setValue(next),
		}),
	);
}

function RawTextApp() {
	const options = useOptions<{ text?: string }>();
	return createElement(Popover, null, createElement(Box, null, options.text));
}

function EntryWithSibling() {
	const [value, setValue] = useState("x");
	const [text, setText] = useState("old");
	return createElement(
		Popover,
		null,
		createElement(Label, { text }),
		createElement(Entry, {
			value,
			onChange: (next: string) => {
				setText("new");
				setValue(next);
			},
		}),
	);
}

function ControlledRowApp({
	type,
	prop,
	event,
	initial,
}: {
	type: unknown;
	prop: string;
	event: string;
	initial: boolean | number;
}) {
	const [value, setValue] = useState(initial);
	return createElement(
		Popover,
		null,
		createElement(type, {
			[prop]: value,
			[event]: (next: boolean | number) => setValue(next),
		}),
	);
}

function Loud() {
	console.log("from-log");
	console.info("from-info");
	console.debug("from-debug");
	return createElement(Indicator, { icon: "view-list-symbolic" });
}

function Boom(): never {
	throw new Error("render boom");
}

function StateDir() {
	return createElement(Indicator, { text: useStateDir() });
}

function mockExit(throwOnExit: boolean): {
	code: () => number | null;
	restore: () => void;
} {
	const original = process.exit;
	let status: number | null = null;
	process.exit = ((code?: number) => {
		status = code ?? 0;
		if (throwOnExit) throw new Error(`node-exit:${status}`);
	}) as typeof process.exit;
	return {
		code: () => status,
		restore: () => {
			process.exit = original;
		},
	};
}

test("applet sdk", async (t) => {
	await t.test("sends queued requests after SDK hello", async () => {
		notify({ summary: "early" });
		const lines = await drive(createElement(Indicator), [hostLine()]);
		assert(lines[0] === '{"t":"hello","v":1}', lines.join("\n"));
		assert(lines[1] === '{"t":"notify","summary":"early"}', lines.join("\n"));
	});

	await t.test(
		"a state change from a timer reaches the host without a host message",
		async () => {
			const lines: string[] = [];
			await session(createElement(Ticking), {
				writeLine: (line) => lines.push(line),
				lines: (async function* () {
					yield hostLine();
					await delay(100);
				})(),
			});
			assert(
				lines.some((line) => line.includes('"after"')),
				lines.join("\n"),
			);
		},
	);

	await t.test("hello fixture matches hello.ndjson", async () => {
		const lines = await drive(createElement(HelloApp), [hostLine()]);
		assertMessages(lines, fixture("hello.ndjson"));
	});

	await t.test("state dir comes from host hello", async () => {
		const lines = await drive(createElement(StateDir), [
			{
				t: "hello",
				v: 1,
				state: "/tmp/glimpse-state",
			},
		]);
		assert(lines[1].includes('"text":"/tmp/glimpse-state"'), lines[1]);
	});

	await t.test("every host fixture line is handled", async () => {
		const messages = fs
			.readFileSync(fixture("host.ndjson"), "utf8")
			.trim()
			.split("\n");
		const lines = await drive(createElement(HelloApp), messages);
		assert(lines.length === 2, lines.join("\n"));
		assert(
			optionsStore.get().enabled === true,
			JSON.stringify(optionsStore.get()),
		);
		assert(
			placementStore.get()?.size === 48,
			JSON.stringify(placementStore.get()),
		);
		assert(openStore.get() === false, "popover stayed open");
	});

	await t.test("coalesces one input chunk into one commit", async () => {
		const lines = await drive(createElement(OpsApp), [
			[
				{ t: "hello", v: 1, options: { step: 0 } },
				{ t: "options", options: { step: 1 } },
				{ t: "options", options: { step: 2 } },
			],
		]);
		const commits = lines.slice(1).map((line) => JSON.parse(line));
		assert(commits.length === 1, JSON.stringify(commits));
		assert(commits[0].ops.length === 2, JSON.stringify(commits));
	});

	await t.test(
		"text children ignore empty values and flatten arrays",
		async () => {
			const lines = await drive(
				createElement(
					Popover,
					null,
					createElement(Label, null, ["Hi ", undefined, ["Ada", false]]),
					createElement(Label, null, ["A", null, true, ["B"]]),
				),
				[hostLine()],
			);
			const children = JSON.parse(lines[1]).ops[0].node.children;
			assert(children[0].props.text === "Hi Ada", JSON.stringify(children));
			assert(children[1].props.text === "AB", JSON.stringify(children));
		},
	);

	await t.test("raw text children update without crashing", async () => {
		const lines = await drive(createElement(RawTextApp), [
			{ t: "hello", v: 1, options: { text: "one" } },
			{ t: "options", options: { text: "two" } },
		]);
		const initial = JSON.parse(lines[1]).ops[0].node.children[0].children[0];
		assert(
			initial.type === "label" && initial.props.text === "one",
			JSON.stringify(initial),
		);
		const change = JSON.parse(lines[2]).ops[0];
		assert(
			change.op === "set" && change.props.text === "two",
			JSON.stringify(change),
		);
	});

	await t.test("wire text uses host caps", async () => {
		const lines = await drive(
			createElement(
				Fragment,
				null,
				createElement(Indicator, {
					badge: "b".repeat(20),
					text: "t".repeat(300),
				}),
				createElement(
					Popover,
					null,
					createElement(Entry, { value: "e".repeat(5000) }),
				),
			),
			[hostLine()],
		);
		const nodes = JSON.parse(lines[1]).ops.map(
			(op: { node: unknown }) => op.node,
		);
		assert(nodes[0].props.badge.length === 8, JSON.stringify(nodes[0].props));
		assert(nodes[0].props.text.length === 256, JSON.stringify(nodes[0].props));
		assert(nodes[1].children[0].props.value.length === 4096, "entry cap");
	});

	await t.test("rejects oversized trees", async () => {
		for (const counts of [[201], [150, 150]]) {
			const exit = mockExit(false);
			try {
				const boxes = counts.map((count) =>
					createElement(
						Box,
						null,
						...Array.from({ length: count }, (_, index) =>
							createElement(Label, { key: String(index), text: "x" }),
						),
					),
				);
				const lines = await drive(createElement(Popover, null, ...boxes), [
					hostLine(),
				]);
				assert(lines.length === 1, lines.join("\n"));
			} finally {
				exit.restore();
			}
			assert(exit.code() === 70, `exit ${exit.code()}`);
		}
	});

	await t.test("rejects outgoing lines over 1 MiB", () => {
		let error: unknown;
		try {
			copy("x".repeat(1_048_576));
		} catch (caught) {
			error = caught;
		}
		assert(
			error instanceof Error && error.name === "ProtocolLimitError",
			String(error),
		);
	});

	await t.test("icon tooltips reach the wire", async () => {
		const lines = await drive(
			createElement(
				Popover,
				null,
				createElement(
					Box,
					null,
					createElement(Image, {
						icon: "dialog-warning-symbolic",
						tooltip: "Warning",
					}),
					createElement(Button, {
						icon: "media-playback-start-symbolic",
						tooltip: "Play",
						onClick: () => {},
					}),
				),
			),
			[hostLine()],
		);
		assertMessages(lines, fixture("tooltip.ndjson"));
	});

	await t.test(
		"widget elements reach the wire with handlers as booleans",
		async () => {
			const lines = await drive(
				createElement(
					Popover,
					null,
					createElement(FactList, { facts: [{ label: "a", value: "b" }] }),
					createElement(ChoiceList, {
						choices: [{ label: "x" }],
						selected: 0,
						onSelect: () => {},
					}),
					createElement(Readout, { value: "5", unit: "C" }),
					createElement(Notice, { title: "n", onActivate: () => {} }),
					createElement(Calendar, {
						year: 2026,
						month: 6,
						events: [{ date: "2026-06-04", colors: ["error"] }],
						onMonth: () => {},
						onSelect: () => {},
					}),
					createElement(Dots, { colors: ["accent"] }),
					createElement(DropDown, { options: ["a", "b"], onSelect: () => {} }),
					createElement(ToggleButton, {
						label: "t",
						active: true,
						onToggle: () => {},
					}),
					createElement(CheckButton, { label: "c", onToggle: () => {} }),
					createElement(RangeBar, { min: 0, max: 10, low: 2, high: 4 }),
					createElement(
						Revealer,
						{ revealed: true },
						createElement(Label, { text: "r" }),
					),
					createElement(
						Stack,
						{ visible: 1 },
						createElement(Label, { text: "0" }),
					),
				),
				[hostLine()],
			);
			const nodes = JSON.parse(lines[1]).ops[0].node.children;
			const types = nodes.map((node: { type: string }) => node.type).join(",");
			assert(
				types ===
					"factlist,choicelist,readout,notice,calendar,dots,dropdown,togglebutton,checkbutton,rangebar,revealer,stack",
				types,
			);
			assert(nodes[1].props.onSelect === true, JSON.stringify(nodes[1].props));
			assert(
				nodes[4].props.onMonth === true &&
					nodes[4].props.events[0].colors[0] === "error",
				JSON.stringify(nodes[4].props),
			);
		},
	);

	await t.test("row family props and children reach the wire", async () => {
		const lines = await drive(
			createElement(
				Popover,
				null,
				createElement(
					ActionRow,
					{
						icon: "go-next-symbolic",
						title: "Action",
						subtitle: "Ready",
						value: "Open",
						tone: "warning",
						inactive: true,
						tooltip: "Open item",
						typography: "monospace",
						busy: true,
						onActivate: () => {},
					},
					createElement(Image, { icon: "go-next-symbolic" }),
				),
				createElement(ToggleRow, {
					icon: "emblem-ok-symbolic",
					title: "Toggle",
					subtitle: "State",
					value: "On",
					tone: "error",
					inactive: true,
					tooltip: "Toggle item",
					typography: "numeric",
					busy: true,
					control: "switch",
					active: true,
					onToggle: () => {},
				}),
				createElement(
					MeterRow,
					{
						icon: "battery-symbolic",
						title: "Meter",
						subtitle: "Charge",
						value: "50%",
						tone: "destructive",
						inactive: true,
						tooltip: "Battery charge",
						typography: "numeric",
						busy: true,
						level: 0.5,
					},
					createElement(Readout, { value: "50", unit: "%" }),
				),
				createElement(
					ExpanderRow,
					{
						icon: "folder-symbolic",
						title: "Details",
						subtitle: "More",
						value: "2",
						tone: "warning",
						inactive: true,
						tooltip: "Show details",
						typography: "numeric",
						busy: true,
						open: true,
						tint: true,
						titleLines: 3,
						onToggle: () => {},
					},
					createElement(
						End,
						null,
						createElement(Image, { icon: "folder-symbolic" }),
					),
					createElement(ActionRow, { title: "Nested" }),
				),
				createElement(
					SliderRow,
					{
						title: "Volume",
						subtitle: "Muted",
						value: 0.5,
						min: 0,
						max: 1,
						step: 0.1,
						tone: "error",
						inactive: true,
						tooltip: "Volume",
						onMove: () => {},
						onChange: () => {},
					},
					createElement(
						Start,
						null,
						createElement(Button, {
							icon: "audio-volume-muted-symbolic",
							tooltip: "Unmute",
							className: "flat",
							onClick: () => {},
						}),
					),
				),
				createElement(
					Row,
					{
						icon: "dialog-warning-symbolic",
						title: "Disk",
						subtitle: "Root",
						value: "91%",
						tone: "warning",
						inactive: true,
						tooltip: "Disk usage",
						typography: "numeric",
					},
					createElement(Readout, { value: "91", unit: "%" }),
				),
			),
			[hostLine()],
		);
		const nodes = JSON.parse(lines[1]).ops[0].node.children;
		const expected = [
			{
				icon: "go-next-symbolic",
				title: "Action",
				subtitle: "Ready",
				value: "Open",
				tone: "warning",
				inactive: true,
				tooltip: "Open item",
				typography: "monospace",
				busy: true,
				onActivate: true,
			},
			{
				icon: "emblem-ok-symbolic",
				title: "Toggle",
				subtitle: "State",
				value: "On",
				tone: "error",
				inactive: true,
				tooltip: "Toggle item",
				typography: "numeric",
				busy: true,
				control: "switch",
				active: true,
				onToggle: true,
			},
			{
				icon: "battery-symbolic",
				title: "Meter",
				subtitle: "Charge",
				value: "50%",
				tone: "destructive",
				inactive: true,
				tooltip: "Battery charge",
				typography: "numeric",
				busy: true,
				level: 0.5,
			},
			{
				icon: "folder-symbolic",
				title: "Details",
				subtitle: "More",
				value: "2",
				tone: "warning",
				inactive: true,
				tooltip: "Show details",
				typography: "numeric",
				busy: true,
				open: true,
				tint: true,
				titleLines: 3,
				onToggle: true,
			},
			{
				title: "Volume",
				subtitle: "Muted",
				value: 0.5,
				min: 0,
				max: 1,
				step: 0.1,
				tone: "error",
				inactive: true,
				tooltip: "Volume",
				onMove: true,
				onChange: true,
			},
			{
				icon: "dialog-warning-symbolic",
				title: "Disk",
				subtitle: "Root",
				value: "91%",
				tone: "warning",
				inactive: true,
				tooltip: "Disk usage",
				typography: "numeric",
			},
		];
		assert(
			nodes.map((node: { type: string }) => node.type).join(",") ===
				"actionrow,togglerow,meterrow,expanderrow,sliderrow,row",
			JSON.stringify(nodes),
		);
		for (let index = 0; index < nodes.length; index++) {
			assert(
				JSON.stringify(canonical(nodes[index].props)) ===
					JSON.stringify(canonical(expected[index])),
				JSON.stringify(nodes[index]),
			);
		}
		assert(nodes[0].children[0].type === "image", JSON.stringify(nodes[0]));
		assert(nodes[2].children[0].type === "readout", JSON.stringify(nodes[2]));
		assert(nodes[3].children[0].type === "end", JSON.stringify(nodes[3]));
		assert(
			nodes[3].children[0].children[0].type === "image",
			JSON.stringify(nodes[3]),
		);
		assert(nodes[3].children[1].type === "actionrow", JSON.stringify(nodes[3]));
		assert(nodes[4].children[0].type === "start", JSON.stringify(nodes[4]));
		assert(
			nodes[4].children[0].children[0].props.onClick === true,
			JSON.stringify(nodes[4]),
		);
		assert(nodes[5].children[0].type === "readout", JSON.stringify(nodes[5]));
	});

	await t.test("series props take 366 items and no more", async () => {
		const series = (length: number) =>
			Array.from({ length }, (_, index) => (index % 5 ? index : null));
		const lines = await drive(
			createElement(
				Popover,
				null,
				createElement(Bars, {
					values: series(366),
					tooltips: series(366).map(String),
				}),
				createElement(Sparkline, { values: series(366) }),
			),
			[hostLine()],
		);
		assert(lines.length === 2, lines.join("\n"));
		const exit = mockExit(false);
		try {
			const refused = await drive(
				createElement(
					Popover,
					null,
					createElement(Bars, { values: series(367) }),
				),
				[hostLine()],
			);
			assert(refused.length === 1, refused.join("\n"));
		} finally {
			exit.restore();
		}
		assert(exit.code() === 70, `exit ${exit.code()}`);
	});

	await t.test("rejects array props over 64 items", async () => {
		const exit = mockExit(false);
		try {
			const options = Array.from({ length: 65 }, (_, index) => String(index));
			const lines = await drive(
				createElement(Popover, null, createElement(DropDown, { options })),
				[hostLine()],
			);
			assert(lines.length === 1, lines.join("\n"));
		} finally {
			exit.restore();
		}
		assert(exit.code() === 70, `exit ${exit.code()}`);
	});

	await t.test("ops fixture matches ops.ndjson", async () => {
		const messages: unknown[] = [
			{
				t: "hello",
				v: 1,
				name: "ops",
				options: { step: 0 },
				placement: {
					output: null,
					position: "bottom",
					orientation: "horizontal",
					zone: "center",
					size: 48,
				},
			},
		];
		for (const step of [1, 2, 3, 4, 5, 6]) {
			messages.push({ t: "options", options: { step } });
		}
		messages.push({
			t: "event",
			id: 3,
			name: "onActivate",
			args: [],
			seq: null,
		});
		messages.push({ t: "options", options: { step: 7 } });
		messages.push({ t: "options", options: { step: 8 } });
		const lines = await drive(createElement(OpsApp), messages);
		assert(lines[0] === '{"t":"hello","v":1}', `first line ${lines[0]}`);
		assertMessages(lines.slice(1), fixture("ops.ndjson"));
	});

	await t.test("requests fixture matches requests.ndjson", () => {
		const lines: string[] = [];
		setEmit((message) => lines.push(JSON.stringify(message)));
		notify({
			summary: "Timer",
			body: "The tomato is done",
			icon: "alarm-symbolic",
			urgency: "critical",
		});
		notify({ summary: "Ping", urgency: "low" });
		notify({ summary: "Saved" });
		copy("buy milk");
		openUri("https://example.com/todos");
		closePopover();
		assertMessages(lines, fixture("requests.ndjson"));
	});

	await t.test("console output goes to stderr", async () => {
		const protocol: string[] = [];
		const errors: Uint8Array[] = [];
		const original = fs.writeSync;
		fs.writeSync = ((
			fd: number,
			data: Uint8Array,
			offset: number,
			length: number,
		) => {
			if (fd !== 2) return original(fd, data, offset, length);
			errors.push(data.subarray(offset, offset + length));
			return length;
		}) as typeof fs.writeSync;
		try {
			redirectConsole();
			const methods = [
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
			];
			for (const method of methods) {
				(console as unknown as Record<string, (...args: unknown[]) => void>)[
					method
				](`from-${method}`);
			}
			const lines = await drive(createElement(Loud), [hostLine()]);
			protocol.push(...lines);
		} finally {
			fs.writeSync = original;
			restoreConsole();
		}
		const stderr = new TextDecoder().decode(
			errors.reduce((all, chunk) => {
				const next = new Uint8Array(all.length + chunk.length);
				next.set(all);
				next.set(chunk, all.length);
				return next;
			}, new Uint8Array()),
		);
		const joined = protocol.join("\n");
		assert(!joined.includes("from-log"), "console.log reached the protocol");
		assert(!joined.includes("from-info"), "console.info reached the protocol");
		assert(
			!joined.includes("from-debug"),
			"console.debug reached the protocol",
		);
		assert(stderr.includes("from-log"), `stderr missed log: ${stderr}`);
		assert(stderr.includes("from-info"), `stderr missed info: ${stderr}`);
		assert(stderr.includes("from-debug"), `stderr missed debug: ${stderr}`);
		for (const method of [
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
		]) {
			assert(stderr.includes(`from-${method}`), `stderr missed ${method}`);
			assert(!joined.includes(`from-${method}`), `protocol contains ${method}`);
		}
	});

	await t.test("mounts only after the host hello", async () => {
		const lines: string[] = [];
		const exit = mockExit(true);
		try {
			await run(createElement(Indicator, { icon: "view-list-symbolic" }), {
				writeLine: (line) => lines.push(line),
				lines: (async function* () {})(),
			});
			throw new Error("run returned without exiting");
		} catch (error) {
			assert(
				error instanceof Error && error.message === "node-exit:0",
				String(error),
			);
		} finally {
			exit.restore();
		}
		assert(exit.code() === 0, `exit ${exit.code()}`);
		assert(
			lines.length === 1 && lines[0] === '{"t":"hello","v":1}',
			lines.join("\n"),
		);
	});

	await t.test("an older seq does not overwrite the entry", async () => {
		const lines = await drive(createElement(EntryApp), [
			hostLine(),
			{ t: "event", id: 2, name: "onChange", args: ["ab"], seq: 5 },
			{ t: "event", id: 2, name: "onChange", args: ["a"], seq: 2 },
			{ t: "event", id: 2, name: "onChange", args: ["abc"], seq: 6 },
		]);
		const sets = lines.filter((line) => line.includes('"op":"set"'));
		assert(sets.length === 2, `sets ${sets.length}\n${lines.join("\n")}`);
		assert(
			sets[0].includes('"value":"ab"') && sets[0].includes('"seq":5'),
			sets[0],
		);
		assert(!sets[0].includes('"a"') || sets[0].includes('"ab"'), sets[0]);
		assert(
			!lines.some((line) => line.includes('"value":"a"')),
			lines.join("\n"),
		);
		assert(
			sets[1].includes('"value":"abc"') && sets[1].includes('"seq":6'),
			sets[1],
		);
	});

	for (const { type, prop, event, initial, next } of [
		{
			type: ToggleRow,
			prop: "active",
			event: "onToggle",
			initial: false,
			next: true,
		},
		{
			type: ExpanderRow,
			prop: "open",
			event: "onToggle",
			initial: false,
			next: true,
		},
		{
			type: SliderRow,
			prop: "value",
			event: "onChange",
			initial: 0,
			next: 0.7,
		},
	]) {
		await t.test(
			`${String(type)} ignores an older controlled event`,
			async () => {
				const lines = await drive(
					createElement(ControlledRowApp, { type, prop, event, initial }),
					[
						hostLine(),
						{ t: "event", id: 2, name: event, args: [next], seq: 5 },
						{ t: "event", id: 2, name: event, args: [initial], seq: 2 },
					],
				);
				const sets = lines
					.slice(2)
					.flatMap((line) => JSON.parse(line).ops)
					.filter((op: { op: string }) => op.op === "set");
				assert(sets.length === 1, JSON.stringify(sets));
				assert(
					sets[0].seq === 5 && sets[0].props[prop] === next,
					JSON.stringify(sets[0]),
				);
			},
		);
	}

	await t.test("a rejected edit confirms the controlled value", async () => {
		const lines = await drive(
			createElement(
				Popover,
				null,
				createElement(Entry, { value: "x", onChange: () => {} }),
			),
			[
				hostLine(),
				{
					t: "event",
					id: 2,
					name: "onChange",
					args: ["no"],
					seq: 4,
				},
			],
		);
		const commit = JSON.parse(lines[2]);
		assert(commit.ops[0].seq === 4, JSON.stringify(commit));
		assert(commit.ops[0].props.value === "x", JSON.stringify(commit));
	});

	await t.test("rejects incompatible host versions", async () => {
		const exit = mockExit(false);
		try {
			const lines = await drive(createElement(Indicator), [
				{
					t: "hello",
					v: 2,
				},
			]);
			assert(lines.length === 1, lines.join("\n"));
		} finally {
			exit.restore();
		}
		assert(exit.code() === 1, `exit ${exit.code()}`);
	});

	await t.test("socket io round trips lines and ends on close", async () => {
		const dir = fs.mkdtempSync(path.join(os.tmpdir(), "glimpse-sock-"));
		const socketPath = path.join(dir, "a.sock");
		const received: string[] = [];
		const server = net.createServer((conn) => {
			conn.setEncoding("utf8");
			conn.on("data", (data: string) => {
				received.push(...data.split("\n").filter(Boolean));
				if (received.length === 1) conn.write(`${hostLine()}\n`);
				if (received.length === 2) conn.end();
			});
		});
		server.listen(socketPath);
		await once(server, "listening");
		const exit = mockExit(false);
		try {
			await session(
				createElement(Indicator, { text: "s" }),
				socketIo(socketPath),
			);
		} finally {
			exit.restore();
			server.close();
			fs.rmSync(dir, { recursive: true, force: true });
		}
		assert(received[0] === '{"t":"hello","v":1}', received.join("\n"));
		assert(
			received.length >= 2 && JSON.parse(received[1]).t === "commit",
			received.join("\n"),
		);
		assert(exit.code() === null, `exit ${exit.code()}`);
	});

	await t.test(
		"socket io exits 0 when the socket is missing or refused",
		async () => {
			const dir = fs.mkdtempSync(path.join(os.tmpdir(), "glimpse-sock-"));
			const stale = path.join(dir, "stale.sock");
			const holder = net.createServer().listen(stale);
			await once(holder, "listening");
			holder.close();
			await once(holder, "close");
			fs.writeFileSync(stale, "");
			for (const target of [path.join(dir, "none.sock"), stale]) {
				const exit = mockExit(false);
				try {
					await session(createElement(Indicator), socketIo(target));
				} finally {
					exit.restore();
				}
				assert(exit.code() === 0, `${target}: exit ${exit.code()}`);
			}
			fs.rmSync(dir, { recursive: true, force: true });
		},
	);

	await t.test(
		"socket io exits 0 when the panel drops the connection",
		async () => {
			const dir = fs.mkdtempSync(path.join(os.tmpdir(), "glimpse-sock-"));
			const socketPath = path.join(dir, "d.sock");
			const server = net.createServer();
			server.listen(socketPath);
			await once(server, "listening");
			const connect = net.Socket.prototype.connect;
			try {
				for (const code of ["EPIPE", "ECONNRESET"]) {
					let socket: net.Socket | undefined;
					net.Socket.prototype.connect = function (
						this: net.Socket,
						...args: Parameters<typeof connect>
					) {
						socket = this;
						return connect.apply(this, args);
					} as typeof connect;
					const accepted = once(server, "connection") as Promise<[net.Socket]>;
					const exit = mockExit(false);
					try {
						const running = session(
							createElement(Indicator),
							socketIo(socketPath),
						);
						net.Socket.prototype.connect = connect;
						const [peer] = await accepted;
						socket!.emit("error", Object.assign(new Error(code), { code }));
						await running;
						socket!.destroy();
						peer.destroy();
					} finally {
						exit.restore();
					}
					assert(exit.code() === 0, `${code}: exit ${exit.code()}`);
				}
			} finally {
				net.Socket.prototype.connect = connect;
				server.close();
				fs.rmSync(dir, { recursive: true, force: true });
			}
		},
	);

	await t.test(
		"run uses the socket when GLIMPSE_APPLET_SOCKET is set",
		async () => {
			const dir = fs.mkdtempSync(path.join(os.tmpdir(), "glimpse-sock-"));
			const socketPath = path.join(dir, "r.sock");
			const received: string[] = [];
			const server = net.createServer((conn) => {
				conn.setEncoding("utf8");
				conn.on("data", (data: string) => {
					received.push(...data.split("\n").filter(Boolean));
					conn.end();
				});
			});
			server.listen(socketPath);
			await once(server, "listening");
			const exit = mockExit(true);
			process.env.GLIMPSE_APPLET_SOCKET = socketPath;
			try {
				await run(createElement(Indicator));
				throw new Error("run returned without exiting");
			} catch (error) {
				assert(
					error instanceof Error && error.message === "node-exit:0",
					String(error),
				);
			} finally {
				delete process.env.GLIMPSE_APPLET_SOCKET;
				exit.restore();
				server.close();
				fs.rmSync(dir, { recursive: true, force: true });
			}
			assert(received[0] === '{"t":"hello","v":1}', received.join("\n"));
		},
	);

	await t.test("stdin batches lines received in one tick", async () => {
		const input = new PassThrough();
		const batches: string[][] = [];
		const reading = (async () => {
			for await (const batch of stdinLines(input)) batches.push(batch);
		})();
		input.end("one\ntwo\n");
		await reading;
		assert(
			batches.length === 1 && batches[0].join(",") === "one,two",
			JSON.stringify(batches),
		);
	});

	await t.test("entry seq stays with its value set", async () => {
		const lines = await drive(createElement(EntryWithSibling), [
			hostLine(),
			{ t: "event", id: 3, name: "onChange", args: ["new"], seq: 9 },
		]);
		const sets = lines.flatMap((line) => {
			const message = JSON.parse(line) as {
				t: string;
				ops?: { op: string; id: number; seq?: number }[];
			};
			return message.t === "commit"
				? (message.ops ?? []).filter((op) => op.op === "set")
				: [];
		});
		assert(
			sets.some((op) => op.id === 2 && op.seq === undefined),
			JSON.stringify(sets),
		);
		assert(
			sets.some((op) => op.id === 3 && op.seq === 9),
			JSON.stringify(sets),
		);
	});

	await t.test("unknown elements reach the host", async () => {
		const lines = await drive(
			createElement(Popover, null, createElement("future-widget")),
			[hostLine()],
		);
		assert(lines[1].includes('"type":"future-widget"'), lines[1]);
	});

	await t.test("a render throw exits 70", async () => {
		const exit = mockExit(false);
		try {
			await drive(createElement(Boom), [hostLine()]);
			await new Promise((resolve) => setTimeout(resolve, 0));
		} finally {
			exit.restore();
		}
		assert(exit.code() === 70, `exit ${exit.code()}`);
	});
});
