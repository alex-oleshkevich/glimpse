import { spawn } from "node:child_process";

import { type AppletEntry, entryStore, nameStore, send } from "./state.ts";

export function notify(message: {
	summary: string;
	body?: string;
	icon?: string;
	urgency?: "low" | "normal" | "critical";
}): void {
	spawnTool("notify-send", notifyArgv(message, nameStore.get(), entryStore.get()));
}

export function notifyArgv(
	message: Parameters<typeof notify>[0],
	name: string,
	entry: AppletEntry | null,
): string[] {
	const urgency = ["low", "normal", "critical"].includes(message.urgency ?? "")
		? message.urgency
		: "normal";
	const args = [`--app-name=${entry?.name || name}`, `--urgency=${urgency}`];
	const icon = message.icon || entry?.icon;
	if (icon) args.push(`--icon=${icon}`);
	if (entry?.id)
		args.push(`--hint=string:desktop-entry:${entry.id.replace(/\.desktop$/, "")}`);
	args.push("--", message.summary);
	if (message.body) args.push(message.body);
	return args;
}

export function spawnTool(program: string, args: string[], input?: string): void {
	let reported = false;
	const report = (error: Error) => {
		if (reported) return;
		reported = true;
		process.stderr.write(`${program}: ${error.message.replace(/[\r\n]/g, " ")}\n`);
	};
	try {
		const child = spawn(program, args, {
			stdio: [input === undefined ? "ignore" : "pipe", "ignore", "ignore"],
		});
		child.on("error", report);
		if (child.stdin) {
			child.stdin.on("error", report);
			child.stdin.write(input);
			child.stdin.end();
		}
	} catch (error) {
		report(error as Error);
	}
}

export function copy(text: string): void {
	spawnTool("wl-copy", [], text);
}

export function openUri(uri: string): void {
	send({ t: "open-uri", uri });
}

export function launch(id: string): void {
	send({ t: "launch", id });
}

export function closePopover(): void {
	send({ t: "close-popover" });
}
