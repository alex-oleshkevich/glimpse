import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { parseCallbackEvent } from "../src/events.js";
import {
  Badge,
  BatteryHero,
  BoxedList,
  ButtonRow,
  Calendar,
  CameraIndicator,
  Choice,
  ChoiceList,
  ChoiceTile,
  CircleBox,
  Column,
  Container,
  DateHero,
  EmptyState,
  EventItem,
  Events,
  ExpanderTile,
  Header,
  Hero,
  KeyValueGrid,
  Label,
  LocationIndicator,
  Meter,
  MicIndicator,
  MutedIndicator,
  PagerItem,
  PagerStrip,
  PanelIndicator,
  PopoverShell,
  Row,
  ScreenCastIndicator,
  Scroll,
  SegmentedTile,
  Separator,
  SliderTile,
  Spinner,
  StatusDot,
  SwitchTile,
  Tile,
  WeatherForecastItem,
  WeatherForecastList,
  WeatherHourlyItem,
  WeatherHourlyStrip,
  WorldClock,
  WorldClockRow,
  type TreeNode,
} from "../src/widgets.js";

const here = dirname(fileURLToPath(import.meta.url));
const sourceFixtures = join(here, "..", "..", "fixtures");
const compiledFixtures = join(here, "..", "..", "..", "fixtures");
const fixturesRoot = existsSync(sourceFixtures) ? sourceFixtures : compiledFixtures;

function load(rel: string): any {
  return JSON.parse(readFileSync(join(fixturesRoot, rel), "utf-8"));
}

function widgets(): Record<string, TreeNode> {
  const label = new Label({ label: "Ready", wrap: true });
  const badge = new Badge({ label: "OK", kind: "success" });
  const status = new StatusDot({ status: "warning" });
  return {
    label,
    header: new Header({ label: "Network" }),
    hero: new Hero({
      id: "vpn",
      icon: "network-vpn-symbolic",
      icon_size: 32,
      title: "VPN",
      subtitle: "Disconnected",
      toggle: false,
      toggle_sensitive: true,
      separator: true,
      trailing: badge,
    }),
    badge,
    "status-dot": status,
    "panel-indicator": new PanelIndicator({
      id: "net",
      icon: "network-wireless-symbolic",
      label: "Wi-Fi",
      active: true,
      extra: status,
    }),
    "empty-state": new EmptyState({ title: "No devices", subtitle: "Connect a device to continue" }),
    spinner: new Spinner(),
    meter: new Meter({ label: "Memory", value: 0.51 }),
    separator: new Separator(),
    scroll: new Scroll({ child: label }),
    row: new Row({ children: [label, badge] }),
    column: new Column({ children: [label, badge] }),
    container: new Container({ children: [label] }),
    "circle-box": new CircleBox({ color: "#336699" }),
    "boxed-list": new BoxedList({ children: [label, badge] }),
    "popover-shell": new PopoverShell({ size: "medium", children: [label], footer: [badge], footer_visible: true }),
    tile: new Tile({
      id: "wifi",
      primary: "Wi-Fi",
      secondary: "Connected",
      left_icon: "network-wireless-symbolic",
      right: badge,
    }),
    "segmented-tile": new SegmentedTile({
      id: "drive",
      primary: "Backup",
      secondary: "Mounted",
      left_icon: "drive-harddisk-symbolic",
      right: badge,
      child: new KeyValueGrid({ rows: [{ key: "Size", value: "1 TB" }] }),
      expanded: true,
    }),
    "button-row": new ButtonRow({ children: [new Tile({ primary: "Refresh" })] }),
    "switch-tile": new SwitchTile({
      id: "bluetooth",
      primary: "Bluetooth",
      secondary: "On",
      left_icon: "bluetooth-active-symbolic",
      active: true,
    }),
    "expander-tile": new ExpanderTile({
      id: "details",
      primary: "Details",
      secondary: "2 items",
      left_icon: "view-list-symbolic",
      child: new Column({ children: [label] }),
      expanded: true,
    }),
    "slider-tile": new SliderTile({
      id: "brightness",
      label: "Brightness",
      left_icon: "display-brightness-symbolic",
      value: 0.6,
      min: 0,
      max: 1,
      step: 0.05,
      page: 0.1,
      digits: 0,
      snap_step: 0.05,
    }),
    "choice-tile": new ChoiceTile({
      id: "balanced",
      primary: "Balanced",
      secondary: "Recommended",
      left_icon: "power-profile-balanced-symbolic",
      selected: true,
    }),
    "choice-list": new ChoiceList({
      id: "profile",
      active: "balanced",
      choices: [
        new Choice({ id: "balanced", primary: "Balanced", secondary: "Recommended", icon: "power-profile-balanced-symbolic" }),
        new Choice({ id: "performance", primary: "Performance", secondary: "Fast", icon: "power-profile-performance-symbolic" }),
      ],
    }),
    "key-value-grid": new KeyValueGrid({ rows: [{ key: "IPv4", value: "10.0.0.42" }] }),
    "pager-item": new PagerItem({ id: 1, label: "1", appearance: "numbers", active: true, occupied: true }),
    "pager-strip": new PagerStrip({
      id: "workspaces",
      items: [
        new PagerItem({ id: 1, label: "1", appearance: "numbers", active: true, occupied: true }),
        new PagerItem({ id: 2, label: "2", appearance: "numbers", inactive: true }),
      ],
    }),
    "camera-indicator": new CameraIndicator({ active: true }),
    "mic-indicator": new MicIndicator({ active: true }),
    "muted-indicator": new MutedIndicator({ active: true }),
    "screencast-indicator": new ScreenCastIndicator({ active: true, timer_text: "01:23" }),
    "location-indicator": new LocationIndicator({ active: true }),
    calendar: new Calendar({ id: "calendar", selected_date: "2026-05-22", event_days: ["2026-05-22", "2026-05-24"] }),
    "battery-hero": new BatteryHero({ icon: "battery-good-symbolic", percentage: "82%", fraction: 0.82, state: "Discharging" }),
    "date-hero": new DateHero({ weekday: "Friday", date: "May 22" }),
    events: new Events({
      date: "2026-05-22",
      events: [new EventItem({ id: "standup", title: "Standup", start: "09:30", end: "09:45" })],
    }),
    "weather-forecast-list": new WeatherForecastList({
      items: [new WeatherForecastItem({ day_name: "Today", icon: "weather-clear-symbolic", condition: "Clear", temperatures: "12 / 20", is_today: true })],
    }),
    "weather-hourly-strip": new WeatherHourlyStrip({
      items: [new WeatherHourlyItem({ time: "12:00", icon: "weather-clear-symbolic", temperature: "18" })],
    }),
    "world-clock": new WorldClock({
      rows: [new WorldClockRow({ name: "UTC", timezone: "UTC", time: "12:00", offset: "+00:00", day_label: "Today" })],
    }),
    "tree-shared-popover": new PopoverShell({
      size: "large",
      children: [
        new Hero({ title: "System", subtitle: "Shared widgets" }),
        new BoxedList({ children: [new SwitchTile({ id: "wifi", primary: "Wi-Fi", active: true })] }),
      ],
    }),
  };
}

test("widgets match golden fixtures", () => {
  for (const [name, widget] of Object.entries(widgets())) {
    assert.deepEqual(widget.toProtocol(), load(`widgets/${name}.json`), name);
  }
});

test("events match golden fixtures", () => {
  const files = readdirSync(join(fixturesRoot, "events")).filter((name) => name.endsWith(".json")).sort();
  for (const file of files) {
    const fixture = load(`events/${file}`);
    assert.deepEqual(parseCallbackEvent(fixture.incoming), fixture.parsed, file);
  }
});
