import test from "node:test";
import assert from "node:assert/strict";

import * as sdk from "../src/index.js";
import {
  Applet,
  Badge,
  Button,
  Column,
  Hero,
  type InitEvent,
  Row,
  Select,
  Spinner,
  StatusDot,
  StatusItem,
  Text,
  parseCallbackEvent,
} from "../src/index.js";

interface DemoState {
  version: string;
  clicks: number;
}

class DemoApplet extends Applet<DemoState> {
  commands: Array<{ command: string; args: string[]; input?: string }> = [];

  protected initialState(): DemoState {
    return { version: "v1", clicks: 0 };
  }

  constructor() {
    super();
    this.onClick("submit", async () => {
      await this.setState({ clicks: this.state.clicks + 1, version: "v2" });
    });
  }

  protected async status(state: DemoState) {
    return [
      new StatusItem({
        id: "demo",
        icon: "demo-symbolic",
        label: state.version,
      }),
    ];
  }

  protected async popover(state: DemoState) {
    return new Column({ children: [
      new Hero({ title: "Demo", subtitle: state.version }),
      new Text(state.version),
      new Button({ id: "submit", label: "Submit" }),
    ] });
  }

  protected async onInit(event: InitEvent): Promise<void> {
    this.state.version = event.instance;
  }

  async drain(): Promise<unknown[]> {
    return this.drainOutgoingForTest();
  }

  async initForTest(instance: string): Promise<void> {
    await (this as any).handleIncoming("init", { instance, options: {} });
  }

  async eventForTest(payload: Record<string, unknown>): Promise<void> {
    await (this as any).handleIncoming("event", payload);
  }

  async copyForTest(text: string): Promise<void> {
    await this.copyToClipboard(text);
  }

  async openUriForTest(uri: string): Promise<void> {
    await this.openUri(uri);
  }

  async notifyForTest(summary: string, body?: string): Promise<void> {
    await this.showNotification(summary, { body });
  }

  async runCommandForTest(command: string[]) {
    return await this.runCommand(command);
  }

  protected async runDesktopCommand(command: string, args: string[], input?: string): Promise<void> {
    this.commands.push({ command, args, input });
  }
}

test("setState updates state and emits protocol messages", async () => {
  const writes: string[] = [];
  const originalWrite = process.stdout.write.bind(process.stdout);
  process.stdout.write = ((chunk: string | Uint8Array) => {
    writes.push(typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf8"));
    return true;
  }) as typeof process.stdout.write;

  try {
    const applet = new DemoApplet();
    await applet.setState({ version: "v2" });
    const drained = await applet.drain();
    const commands = drained.map((message: any) => message.command);
    assert.deepEqual(commands, ["status", "popover"]);
    assert.equal((drained[0] as any).data.items[0].label, "v2");
    assert.equal((drained[0] as any).line, 'status {"items":[{"id":"demo","icon":"demo-symbolic","label":"v2"}]}');
    assert.equal(writes.length, 2);
  } finally {
    process.stdout.write = originalWrite;
  }
});

test("parseCallbackEvent returns typed click event", () => {
  const event = parseCallbackEvent({ id: "submit", type: "click", button: "left" });
  assert.equal(event.event, "click");
  if (event.event !== "click") {
    throw new Error("expected click event");
  }
  assert.equal(event.button, "left");
});

test("parseCallbackEvent returns typed popover event", () => {
  const event = parseCallbackEvent({ id: "popover", type: "open", source: "popover" });
  assert.equal(event.event, "open");
  if (event.event !== "open") {
    throw new Error("expected open event");
  }
  assert.equal(event.open, true);
});

test("select serializes items", () => {
  const node = new Select({
    id: "env",
    items: [{ id: "prod", label: "Production" }],
    selected: 0,
  });
  const payload = node.toProtocol();
  assert.equal(payload.type, "select");
  assert.equal((payload.data as any).items[0].id, "prod");
});

test("row and column serialize as layout protocol types", () => {
  const row = new Row({ children: [] }).toProtocol();
  assert.equal(row.type, "row");
  assert.equal((row.data as any).spacing, 4);
  const column = new Column({ children: [] }).toProtocol();
  assert.equal(column.type, "column");
  assert.equal((column.data as any).spacing, 4);
});

test("section is not a public SDK widget", () => {
  assert.equal("Section" in sdk, false);
});

test("status dot serializes as status protocol type", () => {
  const payload = new StatusDot().toProtocol();
  assert.equal(payload.type, "status");
});

test("spinner serializes with default spinning", () => {
  const payload = new Spinner().toProtocol();
  assert.equal(payload.type, "spinner");
  assert.equal((payload.data as any).spinning, true);
});

test("popover updates are emitted when state changes", async () => {
  const applet = new DemoApplet();
  await applet.drain();
  await applet.setState({ version: "v2" });

  const drained = await applet.drain();
  assert.ok(drained.some((message: any) => message.command === "status"));
  assert.ok(drained.some((message: any) => message.command === "popover"));
  assert.ok(
    drained.some(
      (message: any) =>
        message.command === "popover" &&
        JSON.stringify(message.data).includes("v2"),
    ),
  );
});

test("init event rerenders changed state", async () => {
  const applet = new DemoApplet();
  await applet.drain();
  await applet.initForTest("v3");
  const drained = await applet.drain();
  assert.equal((drained[0] as any).data.items[0].label, "v3");
});

test("variant serializes as semantic protocol value", () => {
  const payload = new Badge({ label: "Warning", variant: "warning" }).toProtocol();
  assert.equal((payload.data as any).variant, "warning");
});

test("desktop helpers run local commands", async () => {
  const applet = new DemoApplet();

  await applet.copyForTest("hello");
  await applet.openUriForTest("https://example.com");
  await applet.notifyForTest("Build complete", "Tests passed");

  assert.deepEqual(applet.commands, [
    { command: "wl-copy", args: [], input: "hello" },
    { command: "xdg-open", args: ["https://example.com"], input: undefined },
    { command: "notify-send", args: ["Build complete", "Tests passed"], input: undefined },
  ]);
});

test("runCommand returns stdout stderr and rc", async () => {
  const applet = new DemoApplet();

  const result = await applet.runCommandForTest([
    process.execPath,
    "-e",
    "process.stdout.write('out\\n'); process.stderr.write('err\\n'); process.exit(7);",
  ]);

  assert.equal(result.stdout, "out\n");
  assert.equal(result.stderr, "err\n");
  assert.equal(result.rc, 7);
});

test("runCommand rejects empty command", async () => {
  const applet = new DemoApplet();

  await assert.rejects(() => applet.runCommandForTest([]), /command must not be empty/);
});
