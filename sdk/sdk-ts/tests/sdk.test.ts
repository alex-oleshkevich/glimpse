import test from "node:test";
import assert from "node:assert/strict";

import {
  Applet,
  Box,
  Button,
  Column,
  Hero,
  Icon,
  type InitEvent,
  Label,
  Row,
  Select,
  SelectOption,
  Spinner,
  StatusDot,
  StatusItem,
  parseCallbackEvent,
} from "../src/index.js";

interface DemoState {
  version: string;
  clicks: number;
}

class DemoApplet extends Applet<DemoState> {
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
        icon: Icon.name("demo-symbolic"),
        label: state.version,
      }),
    ];
  }

  protected async popover(state: DemoState) {
    return Box.vertical([
      new Hero({ title: "Demo", subtitle: state.version }),
      new Label(state.version),
      new Button({ id: "submit", label: "Submit" }),
    ]);
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

  showNotificationForTest(): void {
    this.showNotification({
      summary: "Backup finished",
      body: "42 files synced",
      urgency: "normal",
    });
  }

  openUriForTest(): void {
    this.openUri({ uri: "https://example.com/docs" });
  }

  copyToClipboardForTest(): void {
    this.copyToClipboard({ text: "device-42" });
  }

  dismissNotificationForTest(): void {
    this.dismissNotification({ id: 42 });
  }

  closePopoverForTest(): void {
    this.closePopover();
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
    assert.equal((drained[0] as any).line, 'status {"items":[{"id":"demo","icon":{"name":"demo-symbolic"},"label":"v2"}]}');
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
    items: [new SelectOption("prod", "Production")],
    selected: 0,
  });
  const payload = node.toProtocol();
  assert.equal(payload.type, "select");
  assert.equal((payload.data as any).items[0].id, "prod");
});

test("row and column serialize as layout protocol types", () => {
  const row = new Row({ children: [], spacing: 4 }).toProtocol();
  assert.equal(row.type, "row");
  const column = new Column({ children: [], spacing: 4 }).toProtocol();
  assert.equal(column.type, "column");
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

test("closed popover updates are dropped until opened", async () => {
  const applet = new DemoApplet();
  await applet.drain();
  await applet.setState({ version: "v2" });

  let drained = await applet.drain();
  assert.deepEqual(
    drained.map((message: any) => message.command),
    ["status"],
  );

  await applet.eventForTest({ id: "popover", type: "open", source: "popover" });
  drained = await applet.drain();
  assert.ok(drained.some((message: any) => message.command === "popover"));
});

test("init event rerenders changed state", async () => {
  const applet = new DemoApplet();
  await applet.drain();
  await applet.initForTest("v3");
  const drained = await applet.drain();
  assert.equal((drained[0] as any).data.items[0].label, "v3");
});

test("action helpers emit protocol action messages", async () => {
  const applet = new DemoApplet();
  await applet.drain();

  applet.showNotificationForTest();
  applet.openUriForTest();
  applet.copyToClipboardForTest();
  applet.dismissNotificationForTest();
  applet.closePopoverForTest();

  const drained = await applet.drain();
  assert.deepEqual(
    drained.map((message: any) => message.line),
    [
      'action {"type":"show_notification","arguments":{"summary":"Backup finished","body":"42 files synced","urgency":"normal"}}',
      'action {"type":"open_uri","arguments":{"uri":"https://example.com/docs"}}',
      'action {"type":"copy_to_clipboard","arguments":{"text":"device-42"}}',
      'action {"type":"dismiss_notification","arguments":{"id":42}}',
      'action {"type":"close_popover","arguments":{}}',
    ],
  );
});

test("variant serializes as semantic protocol value", () => {
  const payload = new Label("Warning", { variant: "warning" }).toProtocol();
  assert.equal((payload.data as any).variant, "warning");
});
