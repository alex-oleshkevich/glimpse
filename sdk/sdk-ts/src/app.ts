import { createInterface } from "node:readline";

import {
  type CallbackEvent,
  type ChangeEvent,
  type ClickEvent,
  type InitEvent,
  type InputEvent,
  type PopoverEvent,
  type ScrollEvent,
  type ToggleEvent,
  parseCallbackEvent,
  parseInitEvent,
} from "./events.js";
import { StatusItem } from "./protocol.js";
import { type TreeNode } from "./widgets.js";

type Handler<EventT> = (event: EventT) => void | Promise<void>;

interface OutgoingMessage {
  command: string;
  data: unknown;
  line: string;
}

interface ShowNotificationArgs {
  summary: string;
  body?: string;
  urgency?: string;
}

interface OpenUriArgs {
  uri: string;
}

interface CopyToClipboardArgs {
  text: string;
}

interface DismissNotificationArgs {
  id: number;
}

export abstract class Applet<State extends object> {
  state: State;

  private readonly handlerMap = new Map<string, Handler<CallbackEvent>>();
  private readonly outgoing: OutgoingMessage[] = [];
  private flushPromise: Promise<void> | null = null;
  private renderQueued = false;
  private lastStatus: unknown[] | null = null;
  private lastTree: Record<string, unknown> | null = null;
  private popoverOpen = false;

  protected constructor() {
    this.state = this.initialState();
  }

  protected abstract initialState(): State;

  protected async onStart(): Promise<void> {}

  protected async onInit(_event: InitEvent): Promise<void> {}

  protected async onCallback(_event: CallbackEvent): Promise<void> {}

  protected async status(_state: State): Promise<StatusItem[]> {
    return [];
  }

  protected async popover(_state: State): Promise<TreeNode | null> {
    return null;
  }

  async setState(patch: Partial<State>): Promise<void> {
    this.state = { ...this.state, ...patch };
    await this.scheduleRender();
  }

  onClick(id: string, handler: Handler<ClickEvent>): void {
    this.register("click", id, handler as Handler<CallbackEvent>);
  }

  onScroll(id: string, handler: Handler<ScrollEvent>): void {
    this.register("scroll", id, handler as Handler<CallbackEvent>);
  }

  onInput(id: string, handler: Handler<InputEvent>): void {
    this.register("input", id, handler as Handler<CallbackEvent>);
  }

  onChange(id: string, handler: Handler<ChangeEvent>): void {
    this.register("change", id, handler as Handler<CallbackEvent>);
  }

  onToggle(id: string, handler: Handler<ToggleEvent>): void {
    this.register("toggle", id, handler as Handler<CallbackEvent>);
  }

  isPopoverOpen(): boolean {
    return this.popoverOpen;
  }

  protected showNotification(args: ShowNotificationArgs): void {
    this.emitAction("show_notification", args);
  }

  protected openUri(args: OpenUriArgs): void {
    this.emitAction("open_uri", args);
  }

  protected copyToClipboard(args: CopyToClipboardArgs): void {
    this.emitAction("copy_to_clipboard", args);
  }

  protected dismissNotification(args: DismissNotificationArgs): void {
    this.emitAction("dismiss_notification", args);
  }

  protected closePopover(): void {
    this.emitAction("close_popover", {});
  }

  async run(): Promise<void> {
    process.stdout.on("error", (err: NodeJS.ErrnoException) => {
      if (err.code === "EPIPE") {
        process.exit(0);
      }
    });

    await this.onStart();
    await this.scheduleRender();

    const rl = createInterface({
      input: process.stdin,
      crlfDelay: Infinity,
    });

    for await (const line of rl) {
      if (!line) {
        continue;
      }
      let raw: { command: string; data: unknown } | null;
      try {
        raw = parseLine(line);
      } catch (err) {
        process.stderr.write(`glimpse-sdk: ignoring malformed input: ${err}\n`);
        continue;
      }
      if (raw === null) {
        continue;
      }
      const data = raw.data as Record<string, unknown>;
      try {
        await this.handleIncoming(raw.command, data);
      } catch (err) {
        process.stderr.write(`glimpse-sdk: error handling input: ${err}\n`);
      }
    }
  }

  protected async drainOutgoingForTest(): Promise<OutgoingMessage[]> {
    await this.scheduleRender();
    const drained = [...this.outgoing];
    this.outgoing.length = 0;
    return drained;
  }

  private register(event: string, id: string, handler: Handler<CallbackEvent>): void {
    this.handlerMap.set(`${event}:${id}`, handler);
  }

  private async dispatchCallback(event: CallbackEvent): Promise<void> {
    const handler = this.handlerMap.get(`${event.event}:${event.id}`);
    if (handler !== undefined) {
      await handler(event);
      return;
    }
    await this.onCallback(event);
  }

  private async handleIncoming(type: string, data: Record<string, unknown>): Promise<void> {
    if (type === "init") {
      await this.onInit(parseInitEvent(data));
      await this.scheduleRender();
      return;
    }
    if (type === "event") {
      const event = parseCallbackEvent(data);
      if (isPopoverEvent(event)) {
        this.popoverOpen = event.open;
      }
      await this.dispatchCallback(event);
      await this.scheduleRender();
    }
  }

  private async scheduleRender(): Promise<void> {
    this.renderQueued = true;
    if (this.flushPromise === null) {
      this.flushPromise = Promise.resolve().then(async () => {
        try {
          while (this.renderQueued) {
            this.renderQueued = false;
            await this.flushRender();
          }
        } finally {
          this.flushPromise = null;
        }
      });
    }
    await this.flushPromise;
  }

  private async flushRender(): Promise<void> {
    const statusItems = await this.status(this.state);
    const status = statusItems.map((item) => item.toProtocol());
    if (!deepEqual(status, this.lastStatus)) {
      this.lastStatus = status;
      this.emit("status", { items: status });
    }

    const widget = await this.popover(this.state);
    const tree = { root: widget?.toProtocol() ?? null };
    if (!deepEqual(tree, this.lastTree)) {
      this.lastTree = tree;
      this.emit("popover", tree);
    }
  }

  private emit(command: string, data: unknown): void {
    const line = `${command} ${JSON.stringify(data)}`;
    this.outgoing.push({ command, data, line });
    try {
      process.stdout.write(`${line}\n`);
    } catch (err) {
      const code = (err as NodeJS.ErrnoException)?.code;
      if (code !== "EPIPE") {
        throw err;
      }
    }
  }

  private emitAction(type: string, args: object): void {
    this.emit("action", { type, arguments: args });
  }
}

function parseLine(line: string): { command: string; data: unknown } | null {
  const trimmed = line.trim();
  if (trimmed === "") {
    return null;
  }
  const split = trimmed.search(/\s/);
  if (split < 0) {
    throw new Error("missing command payload");
  }
  return {
    command: trimmed.slice(0, split),
    data: JSON.parse(trimmed.slice(split).trimStart()),
  };
}

function deepEqual(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function isPopoverEvent(event: CallbackEvent): event is PopoverEvent {
  return event.event === "open" || event.event === "close";
}
