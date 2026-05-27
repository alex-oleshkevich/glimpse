export type InlineHandler = (event: unknown) => void | Promise<void>;
export class InlineHandlerRegistry {
  readonly handlers = new Map<string, InlineHandler>();

  generatedId(event: "click" | "toggle" | "change", path: number[]): string {
    const suffix = path.length === 0 ? "root" : path.join(".");
    return `__glimpse:${event}:${suffix}`;
  }

  targetId(event: "click" | "toggle" | "change", id: string | undefined, path: number[]): string {
    return id ?? this.generatedId(event, path);
  }

  add(event: "click" | "toggle" | "change", id: string, handler: InlineHandler): void {
    this.handlers.set(`${event}:${id}`, handler);
  }
}

export type BadgeKind = "default" | "success" | "warning" | "error" | "accent";
export type StatusDotStatus = "neutral" | "success" | "warning" | "error" | "accent";
export type PagerAppearance = "dots" | "numbers";
export type PopoverSize = "small" | "medium" | "large" | "wide";

export interface ProtocolNode {
  type: string;
  data: Record<string, unknown>;
}

export interface CommonOptions {
  visible?: boolean;
  tooltip?: string;
  css_classes?: string[];
  styles?: Record<string, string>;
}

export abstract class WidgetBase {
  protected constructor(protected readonly common: CommonOptions = {}) {}
  abstract readonly type: string;
  abstract data(): Record<string, unknown>;
  bindHandlers(_registry: InlineHandlerRegistry, _path: number[]): void {}

  toProtocol(): ProtocolNode {
    return { type: this.type, data: this.data() };
  }

  protected commonData(): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    if (this.common.visible !== undefined) out.visible = this.common.visible;
    if (this.common.tooltip !== undefined) out.tooltip = this.common.tooltip;
    if (this.common.css_classes?.length) out.css_classes = this.common.css_classes;
    if (this.common.styles && Object.keys(this.common.styles).length > 0) out.styles = this.common.styles;
    return out;
  }
}

const child = (node?: TreeNode): ProtocolNode | undefined => node?.toProtocol();
const children = (nodes: TreeNode[]): ProtocolNode[] => nodes.map((node) => node.toProtocol());

export class Label extends WidgetBase {
  readonly type = "label";
  private readonly options: CommonOptions & { label: string; xalign?: number; wrap?: boolean };
  constructor(options: string | CommonOptions & { label: string; xalign?: number; wrap?: boolean }) {
    const normalized = typeof options === "string" ? { label: options } : options;
    super(normalized);
    this.options = normalized;
  }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...this.commonData(), label: this.options.label };
    if (this.options.xalign !== undefined) out.xalign = this.options.xalign;
    if (this.options.wrap !== undefined) out.wrap = this.options.wrap;
    return out;
  }
}

export class Header extends WidgetBase {
  readonly type = "header";
  constructor(private readonly options: CommonOptions & { label: string }) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), label: this.options.label }; }
}

export class Hero extends WidgetBase {
  readonly type = "hero";
  constructor(private readonly options: CommonOptions & { title: string; subtitle?: string; id?: string; icon?: string; icon_size?: number; toggle?: boolean; toggle_sensitive?: boolean; separator?: boolean; trailing?: TreeNode; on_toggle?: InlineHandler }) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...this.commonData(), title: this.options.title, subtitle: this.options.subtitle ?? "" };
    for (const key of ["id", "icon", "icon_size", "toggle", "toggle_sensitive", "separator"] as const) {
      if (this.options[key] !== undefined) out[key] = this.options[key];
    }
    const trailing = child(this.options.trailing);
    if (trailing) out.trailing = trailing;
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, path: number[]): void {
    if (this.options.on_toggle) {
      this.options.id = registry.targetId("toggle", this.options.id, path);
      registry.add("toggle", this.options.id, this.options.on_toggle);
    }
    this.options.trailing?.bindHandlers(registry, [...path, 0]);
  }
}

export class Badge extends WidgetBase {
  readonly type = "badge";
  constructor(private readonly options: CommonOptions & { label: string; kind?: BadgeKind }) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), label: this.options.label, kind: this.options.kind ?? "default" }; }
}

export class StatusDot extends WidgetBase {
  readonly type = "status_dot";
  constructor(private readonly options: CommonOptions & { status?: StatusDotStatus } = {}) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), status: this.options.status ?? "neutral" }; }
}

export class PanelIndicator extends WidgetBase {
  readonly type = "panel_indicator";
  constructor(private readonly options: CommonOptions & { id?: string; icon?: string; label?: string; active?: boolean; checked?: boolean; needs_attention?: boolean; extra?: TreeNode; on_click?: InlineHandler }) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...this.commonData(), active: this.options.active ?? false, checked: this.options.checked ?? false, needs_attention: this.options.needs_attention ?? false };
    for (const key of ["id", "icon", "label"] as const) if (this.options[key] !== undefined) out[key] = this.options[key];
    const extra = child(this.options.extra);
    if (extra) out.extra = extra;
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, path: number[]): void {
    if (this.options.id && this.options.on_click) registry.add("click", this.options.id, this.options.on_click);
    this.options.extra?.bindHandlers(registry, [...path, 0]);
  }
}

export class EmptyState extends WidgetBase {
  readonly type = "empty_state";
  constructor(private readonly options: CommonOptions & { title: string; subtitle?: string }) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...this.commonData(), title: this.options.title };
    if (this.options.subtitle !== undefined) out.subtitle = this.options.subtitle;
    return out;
  }
}

export class Spinner extends WidgetBase {
  readonly type = "spinner";
  constructor(private readonly options: CommonOptions & { spinning?: boolean } = {}) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), spinning: this.options.spinning ?? true }; }
}

export class Meter extends WidgetBase {
  readonly type = "meter";
  constructor(private readonly options: CommonOptions & { id?: string; icon?: string; label?: string; value?: number; min?: number; max?: number; step?: number; text?: string; interactive?: boolean; on_change?: InlineHandler } = {}) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = {
      ...this.commonData(),
      label: this.options.label ?? "",
      value: this.options.value ?? 0,
      min: this.options.min ?? 0,
      max: this.options.max ?? 1,
      step: this.options.step ?? 0.01,
      interactive: this.options.interactive ?? this.options.on_change !== undefined,
    };
    for (const key of ["id", "icon", "text"] as const) if (this.options[key] !== undefined) out[key] = this.options[key];
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, _path: number[]): void {
    if (this.options.id && this.options.on_change) registry.add("change", this.options.id, this.options.on_change);
  }
}

export class Separator extends WidgetBase {
  readonly type = "separator";
  constructor(common: CommonOptions = {}) { super(common); }
  data(): Record<string, unknown> { return this.commonData(); }
}

export class Scroll extends WidgetBase {
  readonly type = "scroll";
  constructor(private readonly options: CommonOptions & { child: TreeNode }) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), child: this.options.child.toProtocol() }; }
  bindHandlers(registry: InlineHandlerRegistry, path: number[]): void {
    this.options.child.bindHandlers(registry, [...path, 0]);
  }
}

abstract class ChildrenWidget extends WidgetBase {
  constructor(common: CommonOptions & { children?: TreeNode[] } = {}) { super(common); this.kids = common.children ?? []; }
  protected readonly kids: TreeNode[];
  data(): Record<string, unknown> { return { ...this.commonData(), children: children(this.kids) }; }
  bindHandlers(registry: InlineHandlerRegistry, path: number[]): void {
    this.kids.forEach((kid, index) => kid.bindHandlers(registry, [...path, index]));
  }
}

export class Row extends ChildrenWidget { readonly type = "row"; }
export class Column extends ChildrenWidget { readonly type = "column"; }
export class BoxedList extends ChildrenWidget { readonly type = "boxed_list"; }
export class ButtonRow extends ChildrenWidget { readonly type = "button_row"; }

export class CircleBox extends WidgetBase {
  readonly type = "circle_box";
  constructor(private readonly options: CommonOptions & { color: string }) { super(options); }
  data(): Record<string, unknown> {
    const out = this.commonData();
    out.color = this.options.color;
    return out;
  }
}

export class Container extends ChildrenWidget {
  readonly type = "container";
}

export class PopoverShell extends ChildrenWidget {
  readonly type = "popover_shell";
  constructor(private readonly options: CommonOptions & { children?: TreeNode[]; size?: PopoverSize; footer?: TreeNode[]; footer_visible?: boolean } = {}) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...super.data(), size: this.options.size ?? "medium" };
    if ((this.options.footer ?? []).length > 0) out.footer = children(this.options.footer ?? []);
    if (this.options.footer_visible) out.footer_visible = this.options.footer_visible;
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, path: number[]): void {
    super.bindHandlers(registry, path);
    (this.options.footer ?? []).forEach((kid, index) => kid.bindHandlers(registry, [...path, this.kids.length + index]));
  }
}

export class Tile extends WidgetBase {
  readonly type: string = "tile";
  constructor(protected readonly options: CommonOptions & { primary: string; id?: string; secondary?: string; left_icon?: string; left?: TreeNode; right?: TreeNode; on_click?: InlineHandler }) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...this.commonData(), primary: this.options.primary };
    for (const key of ["id", "secondary", "left_icon"] as const) if (this.options[key] !== undefined) out[key] = this.options[key];
    const left = child(this.options.left);
    const right = child(this.options.right);
    if (left) out.left = left;
    if (right) out.right = right;
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, path: number[]): void {
    if (this.options.on_click) {
      this.options.id = registry.targetId("click", this.options.id, path);
      registry.add("click", this.options.id, this.options.on_click);
    }
    this.options.left?.bindHandlers(registry, [...path, 0]);
    this.options.right?.bindHandlers(registry, [...path, 1]);
  }
}

export class SegmentedTile extends Tile {
  readonly type = "segmented_tile";
  constructor(protected readonly options: ConstructorParameters<typeof Tile>[0] & { child?: TreeNode; expanded?: boolean; on_toggle?: InlineHandler }) { super(options); }
  data(): Record<string, unknown> {
    const out = super.data();
    const nested = child(this.options.child);
    if (nested) out.child = nested;
    out.expanded = this.options.expanded ?? false;
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, path: number[]): void {
    super.bindHandlers(registry, path);
    if (this.options.on_toggle) {
      this.options.id = registry.targetId("toggle", this.options.id, path);
      registry.add("toggle", this.options.id, this.options.on_toggle);
    }
    this.options.child?.bindHandlers(registry, [...path, 2]);
  }
}

export class SwitchTile extends WidgetBase {
  readonly type = "switch_tile";
  constructor(private readonly options: CommonOptions & { id: string; primary: string; secondary?: string; left_icon?: string; left?: TreeNode; active?: boolean; on_toggle?: InlineHandler }) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...this.commonData(), id: this.options.id, primary: this.options.primary, active: this.options.active ?? false };
    for (const key of ["secondary", "left_icon"] as const) if (this.options[key] !== undefined) out[key] = this.options[key];
    const left = child(this.options.left);
    if (left) out.left = left;
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, path: number[]): void {
    if (this.options.on_toggle) registry.add("toggle", this.options.id, this.options.on_toggle);
    this.options.left?.bindHandlers(registry, [...path, 0]);
  }
}

export class ExpanderTile extends WidgetBase {
  readonly type = "expander_tile";
  constructor(private readonly options: CommonOptions & { primary: string; id?: string; secondary?: string; left_icon?: string; left?: TreeNode; child?: TreeNode; expanded?: boolean; on_toggle?: InlineHandler }) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...this.commonData(), primary: this.options.primary, expanded: this.options.expanded ?? false };
    for (const key of ["id", "secondary", "left_icon"] as const) if (this.options[key] !== undefined) out[key] = this.options[key];
    const left = child(this.options.left);
    const nested = child(this.options.child);
    if (left) out.left = left;
    if (nested) out.child = nested;
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, path: number[]): void {
    if (this.options.on_toggle) {
      this.options.id = registry.targetId("toggle", this.options.id, path);
      registry.add("toggle", this.options.id, this.options.on_toggle);
    }
    this.options.left?.bindHandlers(registry, [...path, 0]);
    this.options.child?.bindHandlers(registry, [...path, 1]);
  }
}

export class SliderTile extends WidgetBase {
  readonly type = "slider_tile";
  constructor(private readonly options: CommonOptions & { id: string; label?: string; left_icon?: string; left?: TreeNode; value?: number; min?: number; max?: number; step?: number; page?: number; digits?: number; snap_step?: number; on_change?: InlineHandler }) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...this.commonData(), id: this.options.id, value: this.options.value ?? 0, min: this.options.min ?? 0, max: this.options.max ?? 1, step: this.options.step ?? 0.01, page: this.options.page ?? 0.1, digits: this.options.digits ?? 0 };
    for (const key of ["label", "left_icon", "snap_step"] as const) if (this.options[key] !== undefined) out[key] = this.options[key];
    const left = child(this.options.left);
    if (left) out.left = left;
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, path: number[]): void {
    if (this.options.on_change) registry.add("change", this.options.id, this.options.on_change);
    this.options.left?.bindHandlers(registry, [...path, 0]);
  }
}

export class ChoiceTile extends WidgetBase {
  readonly type = "choice_tile";
  constructor(private readonly options: CommonOptions & { primary: string; id?: string; secondary?: string; left_icon?: string; left?: TreeNode; selected?: boolean; on_click?: InlineHandler }) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...this.commonData(), primary: this.options.primary, selected: this.options.selected ?? false };
    for (const key of ["id", "secondary", "left_icon"] as const) if (this.options[key] !== undefined) out[key] = this.options[key];
    const left = child(this.options.left);
    if (left) out.left = left;
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, path: number[]): void {
    if (this.options.on_click) {
      this.options.id = registry.targetId("click", this.options.id, path);
      registry.add("click", this.options.id, this.options.on_click);
    }
    this.options.left?.bindHandlers(registry, [...path, 0]);
  }
}

export class Choice {
  constructor(public readonly options: { id: string; primary: string; secondary?: string; icon?: string }) {}
  toProtocol(): Record<string, unknown> {
    const out: Record<string, unknown> = { id: this.options.id, primary: this.options.primary };
    if (this.options.secondary !== undefined) out.secondary = this.options.secondary;
    if (this.options.icon !== undefined) out.icon = this.options.icon;
    return out;
  }
}

export class ChoiceList extends WidgetBase {
  readonly type = "choice_list";
  constructor(private readonly options: CommonOptions & { id: string; choices?: Choice[]; active?: string; on_change?: InlineHandler }) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...this.commonData(), id: this.options.id, choices: (this.options.choices ?? []).map((choice) => choice.toProtocol()) };
    if (this.options.active !== undefined) out.active = this.options.active;
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, _path: number[]): void {
    if (this.options.on_change) registry.add("change", this.options.id, this.options.on_change);
  }
}

export class KeyValueGrid extends WidgetBase {
  readonly type = "key_value_grid";
  constructor(private readonly options: CommonOptions & { rows?: { key: string; value: string }[] } = {}) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), rows: this.options.rows ?? [] }; }
}

export class PagerItem extends WidgetBase {
  readonly type = "pager_item";
  constructor(private readonly options: CommonOptions & { id: number; label?: string; appearance?: PagerAppearance; active?: boolean; inactive?: boolean; occupied?: boolean; urgent?: boolean; on_click?: InlineHandler }) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), id: this.options.id, label: this.options.label ?? "", appearance: this.options.appearance ?? "dots", active: this.options.active ?? false, inactive: this.options.inactive ?? false, occupied: this.options.occupied ?? false, urgent: this.options.urgent ?? false }; }
  bindHandlers(registry: InlineHandlerRegistry, _path: number[]): void { if (this.options.on_click) registry.add("click", String(this.options.id), this.options.on_click); }
}

export class PagerStrip extends WidgetBase {
  readonly type = "pager_strip";
  constructor(private readonly options: CommonOptions & { id?: string; items?: PagerItem[]; placeholder?: boolean; on_change?: InlineHandler } = {}) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...this.commonData(), placeholder: this.options.placeholder ?? false, items: (this.options.items ?? []).map((item) => item.data()) };
    if (this.options.id !== undefined) out.id = this.options.id;
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, path: number[]): void {
    if (this.options.id && this.options.on_change) registry.add("change", this.options.id, this.options.on_change);
    (this.options.items ?? []).forEach((item, index) => item.bindHandlers(registry, [...path, index]));
  }
}

abstract class ActiveIndicator extends WidgetBase {
  constructor(protected readonly options: CommonOptions & { active?: boolean } = {}) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), active: this.options.active ?? false }; }
}

export class CameraIndicator extends ActiveIndicator { readonly type = "camera_indicator"; }
export class MicIndicator extends ActiveIndicator { readonly type = "mic_indicator"; }
export class MutedIndicator extends ActiveIndicator { readonly type = "muted_indicator"; }
export class LocationIndicator extends ActiveIndicator { readonly type = "location_indicator"; }

export class ScreenCastIndicator extends ActiveIndicator {
  readonly type = "screencast_indicator";
  constructor(protected readonly options: CommonOptions & { active?: boolean; timer_text?: string } = {}) { super(options); }
  data(): Record<string, unknown> {
    const out = super.data();
    if (this.options.timer_text !== undefined) out.timer_text = this.options.timer_text;
    return out;
  }
}

export class Calendar extends WidgetBase {
  readonly type = "calendar";
  constructor(private readonly options: CommonOptions & { selected_date: string; id?: string; event_days?: string[]; on_change?: InlineHandler }) { super(options); }
  data(): Record<string, unknown> {
    const out: Record<string, unknown> = { ...this.commonData(), selected_date: this.options.selected_date, event_days: this.options.event_days ?? [] };
    if (this.options.id !== undefined) out.id = this.options.id;
    return out;
  }
  bindHandlers(registry: InlineHandlerRegistry, _path: number[]): void { if (this.options.id && this.options.on_change) registry.add("change", this.options.id, this.options.on_change); }
}

export class BatteryHero extends WidgetBase {
  readonly type = "battery_hero";
  constructor(private readonly options: CommonOptions & { icon: string; percentage: string; fraction: number; state: string }) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), icon: this.options.icon, percentage: this.options.percentage, fraction: this.options.fraction, state: this.options.state }; }
}

export class DateHero extends WidgetBase {
  readonly type = "date_hero";
  constructor(private readonly options: CommonOptions & { weekday: string; date: string }) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), weekday: this.options.weekday, date: this.options.date }; }
}

export class EventItem {
  constructor(public readonly options: { id: string; title: string; start: string; end?: string; location?: string; all_day?: boolean }) {}
  toProtocol(): Record<string, unknown> {
    const out: Record<string, unknown> = { id: this.options.id, title: this.options.title, start: this.options.start, end: this.options.end ?? "", all_day: this.options.all_day ?? false };
    if (this.options.location !== undefined) out.location = this.options.location;
    return out;
  }
}

export class Events extends WidgetBase {
  readonly type = "events";
  constructor(private readonly options: CommonOptions & { date: string; events?: EventItem[]; loading?: boolean }) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), date: this.options.date, loading: this.options.loading ?? false, events: (this.options.events ?? []).map((event) => event.toProtocol()) }; }
}

export class WeatherForecastItem {
  constructor(public readonly options: { day_name: string; icon: string; condition: string; temperatures: string; is_today?: boolean }) {}
  toProtocol(): Record<string, unknown> { return { day_name: this.options.day_name, icon: this.options.icon, condition: this.options.condition, temperatures: this.options.temperatures, is_today: this.options.is_today ?? false }; }
}

export class WeatherForecastList extends WidgetBase {
  readonly type = "weather_forecast_list";
  constructor(private readonly options: CommonOptions & { items?: WeatherForecastItem[] } = {}) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), items: (this.options.items ?? []).map((item) => item.toProtocol()) }; }
}

export class WeatherHourlyItem {
  constructor(public readonly options: { time: string; icon: string; temperature: string }) {}
  toProtocol(): Record<string, unknown> { return { time: this.options.time, icon: this.options.icon, temperature: this.options.temperature }; }
}

export class WeatherHourlyStrip extends WidgetBase {
  readonly type = "weather_hourly_strip";
  constructor(private readonly options: CommonOptions & { items?: WeatherHourlyItem[] } = {}) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), items: (this.options.items ?? []).map((item) => item.toProtocol()) }; }
}

export class WorldClockRow {
  constructor(public readonly options: { name: string; timezone: string; time: string; offset: string; day_label?: string }) {}
  toProtocol(): Record<string, unknown> {
    const out: Record<string, unknown> = { name: this.options.name, timezone: this.options.timezone, time: this.options.time, offset: this.options.offset };
    if (this.options.day_label !== undefined) out.day_label = this.options.day_label;
    return out;
  }
}

export class WorldClock extends WidgetBase {
  readonly type = "world_clock";
  constructor(private readonly options: CommonOptions & { rows?: WorldClockRow[] } = {}) { super(options); }
  data(): Record<string, unknown> { return { ...this.commonData(), rows: (this.options.rows ?? []).map((row) => row.toProtocol()) }; }
}

export type TreeNode =
  | Row | Column | Container | CircleBox | BoxedList | PopoverShell
  | Label | Header | Hero | Badge | StatusDot | PanelIndicator | EmptyState | Spinner | Meter | Separator | Scroll
  | Tile | SegmentedTile | ButtonRow | SwitchTile | ExpanderTile | SliderTile | ChoiceTile | ChoiceList | KeyValueGrid
  | PagerItem | PagerStrip
  | CameraIndicator | MicIndicator | MutedIndicator | ScreenCastIndicator | LocationIndicator
  | Calendar | BatteryHero | DateHero | Events | WeatherForecastList | WeatherHourlyStrip | WorldClock;
