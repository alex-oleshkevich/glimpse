import { session, socketIo } from "./session.ts";
import { redirectConsole } from "./state.ts";
import type { Io } from "./wire.ts";

redirectConsole();

export { Fragment, jsx, jsxDEV, jsxs } from "./jsx-runtime.ts";
export * from "./elements.ts";
export * from "./hooks.ts";
export * from "./desktop.ts";
export type { Io, Placement } from "./wire.ts";

function defaultIo(): Io {
	const path = process.env.GLIMPSE_APPLET_SOCKET;
	if (!path) {
		console.error(
			"GLIMPSE_APPLET_SOCKET is not set: run the applet under the panel",
		);
		process.exit(1);
	}
	return socketIo(path);
}

export async function run(element: unknown, io?: Io): Promise<void> {
	redirectConsole();
	await session(element, io ?? defaultIo());
	process.exit(0);
}
