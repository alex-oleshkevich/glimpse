import * as react from "react";

import { send, servicesStore } from "./state.ts";

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

export type Player = {
	id: string;
	identity: string;
	desktopEntry: string | null;
	playback: "playing" | "paused" | "stopped";
	current: boolean;
	title: string | null;
	artist: string | null;
	album: string | null;
	lengthUs: number | null;
	positionUs: number;
	positionAt: number;
	rate: number;
	can: {
		play: boolean;
		pause: boolean;
		previous: boolean;
		next: boolean;
		seek: boolean;
	};
};

export type Workspace = {
	id: number;
	index: number | null;
	name: string | null;
	output: string | null;
	active: boolean;
	focused: boolean;
	urgent: boolean;
	windows: number;
};

export type Window = {
	id: number;
	title: string | null;
	appId: string | null;
	workspace: number | null;
	focused: boolean;
	urgent: boolean;
};

export type Output = {
	connector: string;
	label: string | null;
	focused: boolean;
};

export type CompositorSnapshot = {
	name: string | null;
	workspaces: Workspace[];
	windows: Window[];
	outputs: Output[];
};

export type PlayerAction =
	| "play"
	| "pause"
	| "play-pause"
	| "stop"
	| "previous"
	| "next"
	| "raise";

export type Step = "next" | "prev";

export type WorkspaceTarget =
	| { id: number }
	| { index: number }
	| { name: string }
	| Step;

export type WindowTarget = { id: number } | Step;

function useService<T>(name: string): T | null {
	return react.useSyncExternalStore(
		servicesStore.subscribe,
		() => (servicesStore.get()[name] as T | undefined) ?? null,
	);
}

export function useMpris(): Player[] | null {
	return useService<{ players: Player[] }>("mpris")?.players ?? null;
}

export function useCompositor(): CompositorSnapshot | null {
	return useService<CompositorSnapshot>("compositor");
}

export function positionNow(player: Player, now: number = Date.now()): number {
	if (player.playback !== "playing") return player.positionUs;
	const position =
		player.positionUs + (now - player.positionAt) * 1000 * player.rate;
	return player.lengthUs ? Math.min(position, player.lengthUs) : position;
}

export const mpris = {
	control(player: string, action: PlayerAction): void {
		send({ t: "mpris", player, op: "control", action });
	},
	seek(player: string, offsetUs: number): void {
		send({ t: "mpris", player, op: "seek", offset: Math.round(offsetUs) });
	},
	setPosition(player: string, positionUs: number): void {
		send({
			t: "mpris",
			player,
			op: "set-position",
			position: Math.round(positionUs),
		});
	},
};

export const compositor = {
	focusWorkspace(target: WorkspaceTarget): void {
		send({ t: "compositor", op: "focus-workspace", target });
	},
	focusWindow(target: WindowTarget): void {
		send({ t: "compositor", op: "focus-window", target });
	},
	moveWindow(window: number, target: WorkspaceTarget): void {
		send({ t: "compositor", op: "move-window", window, target });
	},
	renameWorkspace(id: number, name: string | null): void {
		send({ t: "compositor", op: "rename-workspace", id, name });
	},
	closeWindow(id: number): void {
		send({ t: "compositor", op: "close-window", id });
	},
};

export function launch(id: string): void {
	send({ t: "launch", id });
}

export function closePopover(): void {
	send({ t: "close-popover" });
}
