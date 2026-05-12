// Golden cross-SDK fixture tests.
//
// Each case builds a widget and asserts its JSON serialization equals the
// corresponding fixture under ../../fixtures/widgets/.
// Event tests parse the canonical incoming payload and assert the documented
// typed event is returned.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";

import {
  ActionMenu,
  ActionMenuItem,
  ActionRow,
  Badge,
  Box,
  Button,
  Card,
  Checkbox,
  Collapsible,
  CollapsibleItem,
  Column,
  Copyable,
  DetailGrid,
  DetailGridItem,
  Dropdown,
  DropdownItem,
  EmptyState,
  Grid,
  GridChild,
  Hero,
  Icon,
  IconWidget,
  Image,
  Item,
  Label,
  MenuItem,
  Meter,
  parseCallbackEvent,
  Progress,
  Row,
  Scale,
  Scroll,
  Section,
  Separator,
  Spinner,
  StatusDot,
  Switch,
  Toast,
  ToastAction,
} from "../src/index.js";

// Compiled path is sdk-ts/dist/tests/golden.test.js -> 3 levels up to sdk/, then /fixtures.
// Source path is sdk-ts/tests/golden.test.ts -> 2 levels up. We pick whichever exists.
import { existsSync } from "node:fs";
const here = import.meta.dirname ?? __dirname;
const candidates = [
  resolve(here, "..", "..", "..", "fixtures"),
  resolve(here, "..", "..", "fixtures"),
];
const fixturesRoot =
  candidates.find((p) => existsSync(p)) ?? candidates[0];

function load(rel: string): unknown {
  const text = readFileSync(join(fixturesRoot, rel), "utf-8");
  return JSON.parse(text);
}

function assertWidget(name: string, widget: { toProtocol(): Record<string, unknown> }): void {
  const expected = load(`widgets/${name}.json`);
  const got = widget.toProtocol();
  assert.deepEqual(got, expected, `fixture mismatch for widgets/${name}.json`);
}

test("widget label-basic", () => {
  assertWidget("label-basic", new Label("Hello"));
});

test("widget label-modifiers", () => {
  assertWidget("label-modifiers", new Label("Hello", { wrap: true, xalign: 0.5, selectable: true }));
});

test("widget button-basic", () => {
  assertWidget("button-basic", new Button({ id: "go", label: "Go" }));
});

test("widget button-with-icon", () => {
  assertWidget(
    "button-with-icon",
    new Button({ id: "go", label: "Go", icon: Icon.name("go-symbolic") }),
  );
});

test("widget button-icon-only", () => {
  assertWidget(
    "button-icon-only",
    new Button({ id: "go", icon: Icon.name("go-symbolic") }),
  );
});

test("widget switch-on", () => {
  assertWidget("switch-on", new Switch({ id: "vpn", label: "VPN", active: true }));
});

test("widget switch-off", () => {
  assertWidget("switch-off", new Switch({ id: "vpn" }));
});

test("widget checkbox-on", () => {
  assertWidget("checkbox-on", new Checkbox({ id: "autostart", label: "Run at login", active: true }));
});

test("widget scale", () => {
  assertWidget("scale", new Scale({ id: "brightness", min: 0, max: 1, step: 0.05, value: 0.6 }));
});

test("widget dropdown", () => {
  assertWidget(
    "dropdown",
    new Dropdown({
      id: "env",
      items: [new DropdownItem("prod", "Production"), new DropdownItem("stage", "Staging")],
      selected: 0,
    }),
  );
});

test("widget dropdown-empty", () => {
  assertWidget("dropdown-empty", new Dropdown({ id: "env" }));
});

test("widget badge", () => {
  assertWidget("badge", new Badge({ label: "42%" }));
});

test("widget badge-success-variant", () => {
  assertWidget("badge-success-variant", new Badge({ label: "OK", variant: "success" }));
});

test("widget hero-basic", () => {
  assertWidget("hero-basic", new Hero({ title: "Counter", subtitle: "Value: 0" }));
});

test("widget hero-with-icon", () => {
  assertWidget(
    "hero-with-icon",
    new Hero({ title: "VPN", subtitle: "Connected", icon: Icon.name("network-vpn-symbolic") }),
  );
});

test("widget progress", () => {
  assertWidget("progress", new Progress({ value: 0.7, max: 1 }));
});

test("widget progress-with-text", () => {
  assertWidget(
    "progress-with-text",
    new Progress({ value: 0.7, max: 1, show_text: true, text: "70%" }),
  );
});

test("widget spinner-default", () => {
  assertWidget("spinner-default", new Spinner());
});

test("widget spinner-stopped", () => {
  assertWidget("spinner-stopped", new Spinner({ spinning: false }));
});

test("widget status-dot", () => {
  assertWidget("status-dot", new StatusDot());
});

test("widget status-dot-warning", () => {
  assertWidget("status-dot-warning", new StatusDot({ variant: "warning" }));
});

test("widget icon", () => {
  assertWidget(
    "icon",
    new IconWidget(Icon.name("network-wireless-symbolic"), { pixel_size: 24 }),
  );
});

test("widget image-by-name", () => {
  assertWidget("image-by-name", new Image(Icon.name("user-info-symbolic")));
});

test("widget image-by-path", () => {
  assertWidget(
    "image-by-path",
    new Image(Icon.path("/home/me/avatar.png"), { pixel_size: 64 }),
  );
});

test("widget separator", () => {
  assertWidget("separator", new Separator());
});

test("widget box-vertical", () => {
  assertWidget("box-vertical", Box.vertical([], 8));
});

test("widget box-horizontal", () => {
  assertWidget("box-horizontal", Box.horizontal([], 4));
});

test("widget row", () => {
  assertWidget("row", new Row({ spacing: 8, children: [] }));
});

test("widget column", () => {
  assertWidget("column", new Column({ spacing: 8, children: [] }));
});

test("widget grid", () => {
  assertWidget(
    "grid",
    new Grid({
      row_spacing: 4,
      column_spacing: 4,
      children: [
        new GridChild(0, 0, new Label("A")),
        new GridChild(0, 1, new Label("B"), 2, 1),
      ],
    }),
  );
});

test("widget scroll", () => {
  assertWidget("scroll", new Scroll(new Label("scrollable")));
});

test("widget card", () => {
  assertWidget("card", new Card({ children: [new Label("in card")] }));
});

test("widget card-empty", () => {
  assertWidget("card-empty", new Card({ children: [] }));
});

test("widget section-basic", () => {
  assertWidget(
    "section-basic",
    new Section({ title: "System", body: [new Label("uptime")] }),
  );
});

test("widget section-empty-body", () => {
  assertWidget("section-empty-body", new Section({ title: "Empty", body: [] }));
});

test("widget collapsible-closed", () => {
  assertWidget(
    "collapsible-closed",
    new Collapsible({ title: "Advanced", expanded: false, body: [] }),
  );
});

test("widget collapsible-open-with-body", () => {
  assertWidget(
    "collapsible-open-with-body",
    new Collapsible({ title: "Advanced", expanded: true, body: [new Label("inside")] }),
  );
});

test("widget item-basic", () => {
  assertWidget("item-basic", new Item({ label: "Plain" }));
});

test("widget item-clickable", () => {
  assertWidget("item-clickable", new Item({ id: "run", label: "Run", clickable: true }));
});

test("widget item-with-menu", () => {
  assertWidget(
    "item-with-menu",
    new Item({
      id: "wifi-home",
      label: "home-5G",
      clickable: true,
      menu: [
        new MenuItem({ id: "forget", label: "Forget" }),
        new MenuItem({ id: "details", label: "Details", enabled: false }),
      ],
    }),
  );
});

test("widget collapsible-item", () => {
  assertWidget("collapsible-item", new CollapsibleItem({ label: "Devices", expanded: false, body: [] }));
});

test("widget action-row", () => {
  assertWidget("action-row", new ActionRow({ id: "go", title: "Connect" }));
});

test("widget action-row-with-meta", () => {
  assertWidget(
    "action-row-with-meta",
    new ActionRow({
      id: "go",
      title: "Connect",
      subtitle: "wg0",
      meta: "4 routes",
      icon: Icon.name("network-vpn-symbolic"),
    }),
  );
});

test("widget action-menu", () => {
  assertWidget(
    "action-menu",
    new ActionMenu({
      header: "Power profile",
      items: [
        new ActionMenuItem({ id: "saver", label: "Power Saver", checked: false }),
        new ActionMenuItem({ id: "balanced", label: "Balanced", checked: true }),
      ],
    }),
  );
});

test("widget action-menu-empty", () => {
  assertWidget("action-menu-empty", new ActionMenu({ items: [] }));
});

test("widget detail-grid", () => {
  assertWidget(
    "detail-grid",
    new DetailGrid({
      rows: [
        new DetailGridItem("SSID", "home-5G"),
        new DetailGridItem("IPv4", "10.0.0.42"),
      ],
    }),
  );
});

test("widget detail-grid-empty", () => {
  assertWidget("detail-grid-empty", new DetailGrid({ rows: [] }));
});

test("widget empty-state", () => {
  assertWidget("empty-state", new EmptyState({ title: "Nothing here" }));
});

test("widget empty-state-with-subtitle", () => {
  assertWidget(
    "empty-state-with-subtitle",
    new EmptyState({ title: "Nothing here", subtitle: "Plug in a device." }),
  );
});

test("widget meter", () => {
  assertWidget("meter", new Meter({ label: "Memory", value: 0.51, min: 0, max: 1, step: 0.01 }));
});

test("widget meter-interactive", () => {
  assertWidget(
    "meter-interactive",
    new Meter({
      icon: Icon.name("audio-volume-medium-symbolic"),
      label: "Volume",
      value: 0.42,
      min: 0,
      max: 1,
      step: 0.01,
      text: "42%",
      interactive: true,
    }),
  );
});

test("widget copyable", () => {
  assertWidget("copyable", new Copyable({ label: "IPv4", value: "10.0.0.42" }));
});

test("widget toast", () => {
  assertWidget("toast", new Toast({ title: "Saved" }));
});

test("widget toast-with-action", () => {
  assertWidget(
    "toast-with-action",
    new Toast({
      icon: Icon.name("dialog-warning-symbolic"),
      title: "Update available",
      message: "Version 0.8 is available.",
      action: new ToastAction("update", "Update"),
    }),
  );
});

test("widget common-props-all", () => {
  assertWidget(
    "common-props-all",
    new Label("marked", {
      id: "marked",
      visible: false,
      hexpand: true,
      vexpand: true,
      halign: "center",
      valign: "end",
      tooltip: "details",
      variant: "warning",
    }),
  );
});

test("widget tree-hero-column-section", () => {
  assertWidget(
    "tree-hero-column-section",
    new Column({
      spacing: 8,
      children: [
        new Hero({ title: "Counter", subtitle: "Value: 0" }),
        new Section({
          title: "Controls",
          body: [new Label("Current"), new Button({ id: "increment", label: "Increment" })],
        }),
      ],
    }),
  );
});

test("widget tree-card-with-grid", () => {
  assertWidget(
    "tree-card-with-grid",
    new Card({
      children: [
        new Grid({
          row_spacing: 4,
          column_spacing: 8,
          children: [
            new GridChild(0, 0, new Label("K")),
            new GridChild(0, 1, new Badge({ label: "V" })),
          ],
        }),
      ],
    }),
  );
});

// ---------- events ----------

interface EventFixture {
  incoming: Record<string, unknown>;
  parsed: Record<string, unknown>;
}

function loadEvent(name: string): EventFixture {
  return load(`events/${name}.json`) as EventFixture;
}

test("event click-left", () => {
  const f = loadEvent("click-left");
  const e = parseCallbackEvent(f.incoming);
  assert.equal(e.event, "click");
  assert.equal(e.id, f.parsed.id);
  if (e.event === "click") {
    assert.equal(e.button, f.parsed.button);
  }
});

test("event click-no-button", () => {
  const f = loadEvent("click-no-button");
  const e = parseCallbackEvent(f.incoming);
  assert.equal(e.event, "click");
  if (e.event === "click") {
    assert.equal(e.button, undefined);
  }
});

test("event scroll-down", () => {
  const f = loadEvent("scroll-down");
  const e = parseCallbackEvent(f.incoming);
  assert.equal(e.event, "scroll");
  if (e.event === "scroll") {
    assert.equal(e.delta_y, f.parsed.delta_y);
  }
});

test("event input", () => {
  const f = loadEvent("input");
  const e = parseCallbackEvent(f.incoming);
  assert.equal(e.event, "input");
  if (e.event === "input") {
    assert.equal(e.text, f.parsed.text);
  }
});

test("event toggle-active-true", () => {
  const f = loadEvent("toggle-active-true");
  const e = parseCallbackEvent(f.incoming);
  assert.equal(e.event, "toggle");
  if (e.event === "toggle") {
    assert.equal(e.value, true);
  }
});

test("event toggle-active-false", () => {
  const f = loadEvent("toggle-active-false");
  const e = parseCallbackEvent(f.incoming);
  assert.equal(e.event, "toggle");
  if (e.event === "toggle") {
    assert.equal(e.value, false);
  }
});

test("event toggle-via-value-true", () => {
  const f = loadEvent("toggle-via-value-true");
  const e = parseCallbackEvent(f.incoming);
  assert.equal(e.event, "toggle");
  if (e.event === "toggle") {
    assert.equal(e.value, true);
  }
});

test("event toggle-numeric-value-is-false", () => {
  const f = loadEvent("toggle-numeric-value-is-false");
  const e = parseCallbackEvent(f.incoming);
  assert.equal(e.event, "toggle");
  if (e.event === "toggle") {
    assert.equal(e.value, false);
  }
});

test("event change-scale", () => {
  const f = loadEvent("change-scale");
  const e = parseCallbackEvent(f.incoming);
  assert.equal(e.event, "change");
  if (e.event === "change") {
    assert.deepEqual(e.value, f.parsed.value);
  }
});

test("event change-dropdown", () => {
  const f = loadEvent("change-dropdown");
  const e = parseCallbackEvent(f.incoming);
  assert.equal(e.event, "change");
  if (e.event === "change") {
    assert.deepEqual(e.value, f.parsed.value);
  }
});

test("event popover-open", () => {
  const f = loadEvent("popover-open");
  const e = parseCallbackEvent(f.incoming);
  assert.equal(e.event, "open");
  if (e.event === "open" || e.event === "close") {
    assert.equal(e.open, true);
  }
});

test("event popover-close", () => {
  const f = loadEvent("popover-close");
  const e = parseCallbackEvent(f.incoming);
  assert.equal(e.event, "close");
  if (e.event === "close") {
    assert.equal(e.open, false);
  }
});
