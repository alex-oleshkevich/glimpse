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
  ActionItem,
  Badge,
  Box,
  Button,
  Card,
  Checkbox,
  Column,
  Copyable,
  EmptyState,
  Expander,
  Grid,
  GridChild,
  Hero,
  Icon,
  Image,
  Item,
  Label,
  LevelBar,
  LinkButton,
  ListBox,
  MenuButton,
  Meter,
  Overlay,
  PagerItem,
  PagerStrip,
  Picture,
  parseCallbackEvent,
  Progress,
  PropertyList,
  Row,
  Scroll,
  Section,
  Select,
  SelectOption,
  Separator,
  Slider,
  Spinner,
  StatusDot,
  Switch,
  ToggleButton,
  TreeExpander,
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
    new Button({ id: "go", label: "Go", icon: "go-symbolic" }),
  );
});

test("widget button-icon-only", () => {
  assertWidget(
    "button-icon-only",
    new Button({ id: "go", icon: "go-symbolic" }),
  );
});

test("widget button-primary", () => {
  assertWidget("button-primary", new Button({ id: "go", label: "Go", variant: "primary" }));
});

test("widget button-disabled", () => {
  assertWidget("button-disabled", new Button({ id: "go", label: "Go", enabled: false }));
});

test("widget link-button", () => {
  assertWidget("link-button", new LinkButton({ uri: "https://example.com" }));
});

test("widget link-button-label", () => {
  assertWidget("link-button-label", new LinkButton({ uri: "https://example.com/docs", label: "Docs" }));
});

test("widget expander", () => {
  assertWidget("expander", new Expander({ label: "Details", child: new Label("More") }));
});

test("widget expander-expanded", () => {
  assertWidget("expander-expanded", new Expander({ label: "Details", expanded: true, child: new Label("More") }));
});

test("widget overlay", () => {
  assertWidget("overlay", new Overlay({ child: new Label("Base"), overlays: [new Badge({ label: "Top" })] }));
});

test("widget list-box", () => {
  assertWidget("list-box", new ListBox({ children: [new Label("First"), new Badge({ label: "Second" })] }));
});

test("widget level-bar", () => {
  assertWidget("level-bar", new LevelBar({ value: 0.7, min: 0, max: 1, mode: "continuous" }));
});

test("widget tree-expander", () => {
  assertWidget(
    "tree-expander",
    new TreeExpander({
      child: new Label("Nested"),
      hide_expander: true,
      indent_for_depth: true,
      indent_for_icon: true,
    }),
  );
});

test("widget menu-button", () => {
  assertWidget(
    "menu-button",
    new MenuButton({
      label: "More",
      icon: "open-menu-symbolic",
      popover: new Label("Menu content"),
    }),
  );
});

test("widget switch-on", () => {
  assertWidget("switch-on", new Switch({ id: "vpn", label: "VPN", active: true }));
});

test("widget switch-off", () => {
  assertWidget("switch-off", new Switch({ id: "vpn" }));
});

test("widget toggle-button-on", () => {
  assertWidget("toggle-button-on", new ToggleButton({ id: "wifi", label: "Wi-Fi", active: true }));
});

test("widget toggle-button-off", () => {
  assertWidget("toggle-button-off", new ToggleButton({ id: "wifi" }));
});

test("widget checkbox-on", () => {
  assertWidget("checkbox-on", new Checkbox({ id: "autostart", label: "Run at login", active: true }));
});

test("widget slider", () => {
  assertWidget("slider", new Slider({ id: "brightness", min: 0, max: 1, step: 0.05, value: 0.6 }));
});

test("widget select", () => {
  assertWidget(
    "select",
    new Select({
      id: "env",
      items: [new SelectOption("prod", "Production"), new SelectOption("stage", "Staging")],
      selected: 0,
    }),
  );
});

test("widget select-empty", () => {
  assertWidget("select-empty", new Select({ id: "env" }));
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

test("widget pager-item-number-active", () => {
  assertWidget(
    "pager-item-number-active",
    new PagerItem({ id: "workspace-1", appearance: "numbers", label: "1", active: true }),
  );
});

test("widget pager-strip", () => {
  assertWidget(
    "pager-strip",
    new PagerStrip({
      items: [
        new PagerItem({ id: "workspace-1", appearance: "numbers", label: "1", active: true }),
        new PagerItem({ id: "workspace-2", appearance: "numbers", label: "2", occupied: true }),
        new PagerItem({ id: "workspace-3", appearance: "dots", urgent: true }),
      ],
    }),
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

test("widget picture", () => {
  assertWidget("picture", new Picture({ path: "/home/me/photo.png" }));
});

test("widget picture-content-fit", () => {
  assertWidget("picture-content-fit", new Picture({ path: "/home/me/photo.png", content_fit: "cover" }));
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
    new Section({ title: "System", children: [new Label("uptime")] }),
  );
});

test("widget section-empty-children", () => {
  assertWidget("section-empty-children", new Section({ title: "Empty", children: [] }));
});

test("widget property-list", () => {
  assertWidget(
    "property-list",
    new PropertyList({
      rows: {
        IPv4: "10.0.0.42",
        SSID: "home-5G",
      },
    }),
  );
});

test("widget property-list-title", () => {
  assertWidget(
    "property-list-title",
    new PropertyList({
      title: "Network",
      rows: {
        IPv4: "10.0.0.42",
        SSID: "home-5G",
      },
    }),
  );
});

test("widget property-list-empty", () => {
  assertWidget("property-list-empty", new PropertyList());
});

test("widget item", () => {
  assertWidget("item", new Item({ label: "Wi-Fi" }));
});

test("widget item-with-right", () => {
  assertWidget(
    "item-with-right",
    new Item({
      icon: "network-wireless-symbolic",
      label: "Wi-Fi",
      sublabel: "Connected",
      right: new Badge({ label: "home-5G" }),
    }),
  );
});

test("widget action-item", () => {
  assertWidget("action-item", new ActionItem({ id: "wifi", label: "Wi-Fi" }));
});

test("widget action-item-with-right", () => {
  assertWidget(
    "action-item-with-right",
    new ActionItem({
      id: "wifi",
      icon: "network-wireless-symbolic",
      label: "Wi-Fi",
      sublabel: "Connected",
      right: new Badge({ label: "home-5G" }),
    }),
  );
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
      id: "volume",
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
          children: [new Label("Current"), new Button({ id: "increment", label: "Increment" })],
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
