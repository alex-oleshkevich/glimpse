export type Align = "fill" | "start" | "end" | "center" | "baseline";
export type Orientation = "horizontal" | "vertical";
export type Variant = "normal" | "muted" | "accent" | "success" | "warning" | "danger";
export const Space = {
  NONE: "none",
  XXS: "xxs",
  XS: "xs",
  SM: "sm",
  MD: "md",
  LG: "lg",
} as const;
export type Space = (typeof Space)[keyof typeof Space];
export const Color = {
  BG: "bg",
  FG: "fg",
  SURFACE: "surface",
  SURFACE_RAISED: "surface_raised",
  BORDER: "border",
  MUTED_FG: "muted_fg",
  ACCENT: "accent",
  ACCENT_FG: "accent_fg",
  SUCCESS: "success",
  SUCCESS_FG: "success_fg",
  WARNING: "warning",
  WARNING_FG: "warning_fg",
  DANGER: "danger",
  DANGER_FG: "danger_fg",
} as const;
export type Color = (typeof Color)[keyof typeof Color];
export const Radius = {
  NONE: "none",
  SM: "sm",
  MD: "md",
  LG: "lg",
  PILL: "pill",
} as const;
export type Radius = (typeof Radius)[keyof typeof Radius];
export const FontSize = {
  XXS: "xxs",
  XS: "xs",
  SM: "sm",
  MD: "md",
  BASE: "base",
  LG: "lg",
  XL: "xl",
} as const;
export type FontSize = (typeof FontSize)[keyof typeof FontSize];
export const FontWeight = {
  NORMAL: "normal",
  MEDIUM: "medium",
  SEMIBOLD: "semibold",
  BOLD: "bold",
} as const;
export type FontWeight = (typeof FontWeight)[keyof typeof FontWeight];
export const TextAlign = {
  LEFT: "left",
  CENTER: "center",
  RIGHT: "right",
} as const;
export type TextAlign = (typeof TextAlign)[keyof typeof TextAlign];
export const BorderWidth = {
  NONE: "none",
  THIN: "thin",
  MEDIUM: "medium",
  THICK: "thick",
} as const;
export type BorderWidth = (typeof BorderWidth)[keyof typeof BorderWidth];
export type ButtonVariant = "primary" | "secondary" | "compact" | "flat" | "danger";
export type PagerAppearance = "dots" | "numbers";
export type ContentFit = "fill" | "contain" | "cover" | "scale_down";
export type LevelBarMode = "continuous" | "discrete";
export type StatusVariant = "success" | "warning" | "danger";

export interface WidgetNode {
  toProtocol(): Record<string, unknown>;
}

export interface CommonProps {
  visible?: boolean;
  hexpand?: boolean;
  vexpand?: boolean;
  halign?: Align;
  valign?: Align;
  tooltip?: string;
  css_classes?: string[];
  styles?: Record<string, string>;
}

function applyCommonProps(
  payload: Record<string, unknown>,
  props: CommonProps,
): Record<string, unknown> {
  if (props.visible !== undefined) payload.visible = props.visible;
  if (props.hexpand !== undefined) payload.hexpand = props.hexpand;
  if (props.vexpand !== undefined) payload.vexpand = props.vexpand;
  if (props.halign !== undefined) payload.halign = props.halign;
  if (props.valign !== undefined) payload.valign = props.valign;
  if (props.tooltip !== undefined) payload.tooltip = props.tooltip;
  if (props.css_classes !== undefined && props.css_classes.length > 0) payload.css_classes = props.css_classes;
  if (props.styles !== undefined && Object.keys(props.styles).length > 0) payload.styles = props.styles;
  return payload;
}

abstract class WidgetBase implements WidgetNode {
  protected constructor(protected readonly common: CommonProps = {}) {}

  protected withCommon(payload: Record<string, unknown>): Record<string, unknown> {
    return applyCommonProps(payload, this.common);
  }

  abstract toProtocol(): Record<string, unknown>;
}

export class Text extends WidgetBase {
  constructor(
    private readonly text: string,
    private readonly options: CommonProps & {
      color?: Color;
      size?: FontSize;
      weight?: FontWeight;
      align?: TextAlign;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ text: this.text });
    if (this.options.color !== undefined) payload.color = this.options.color;
    if (this.options.size !== undefined) payload.size = this.options.size;
    if (this.options.weight !== undefined) payload.weight = this.options.weight;
    if (this.options.align !== undefined) payload.align = this.options.align;
    return { type: "text", data: payload };
  }
}

export class Icon extends WidgetBase {
  constructor(
    public readonly icon: string,
    private readonly options: CommonProps & { pixel_size?: number } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ icon: this.icon });
    if (this.options.pixel_size !== undefined) payload.pixel_size = this.options.pixel_size;
    return { type: "icon", data: payload };
  }
}

export class Progress extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      value: number;
      max?: number;
      show_text?: boolean;
      text?: string;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      value: this.options.value,
      max: this.options.max ?? 1,
    });
    if (this.options.show_text !== undefined) payload.show_text = this.options.show_text;
    if (this.options.text !== undefined) payload.text = this.options.text;
    return { type: "progress", data: payload };
  }
}

export class LevelBar extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      value: number;
      min?: number;
      max?: number;
      mode?: LevelBarMode;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "level_bar",
      data: this.withCommon({
        value: this.options.value,
        min: this.options.min ?? 0,
        max: this.options.max ?? 1,
        mode: this.options.mode ?? "continuous",
      }),
    };
  }
}

export class Picture extends WidgetBase {
  constructor(
    public readonly options: CommonProps & { path: string; content_fit?: ContentFit },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ path: this.options.path });
    if (this.options.content_fit !== undefined) payload.content_fit = this.options.content_fit;
    return { type: "picture", data: payload };
  }
}

export class Button extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      id: string;
      label?: string;
      icon?: string;
      enabled?: boolean;
      variant?: ButtonVariant;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ id: this.options.id });
    if (this.options.label !== undefined) payload.label = this.options.label;
    if (this.options.icon !== undefined) payload.icon = this.options.icon;
    if (this.options.enabled !== undefined) payload.enabled = this.options.enabled;
    if (this.options.variant !== undefined) payload.variant = this.options.variant;
    return { type: "button", data: payload };
  }
}

export class LinkButton extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      uri: string;
      label?: string;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ uri: this.options.uri });
    if (this.options.label !== undefined) payload.label = this.options.label;
    return { type: "link_button", data: payload };
  }
}

export class Expander extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      label: string;
      child: TreeNode;
      expanded?: boolean;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "expander",
      data: this.withCommon({
        label: this.options.label,
        expanded: this.options.expanded ?? false,
        child: this.options.child.toProtocol(),
      }),
    };
  }
}

export class Switch extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      id: string;
      label?: string;
      active?: boolean;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ id: this.options.id, active: this.options.active ?? false });
    if (this.options.label !== undefined) payload.label = this.options.label;
    return { type: "switch", data: payload };
  }
}

export class ToggleButton extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      id: string;
      label?: string;
      icon?: string;
      active?: boolean;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ id: this.options.id, active: this.options.active ?? false });
    if (this.options.label !== undefined) payload.label = this.options.label;
    if (this.options.icon !== undefined) payload.icon = this.options.icon;
    return { type: "toggle_button", data: payload };
  }
}

export class Slider extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      id: string;
      min?: number;
      max?: number;
      step?: number;
      value?: number;
      orientation?: Orientation;
      draw_value?: boolean;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      id: this.options.id,
      min: this.options.min ?? 0,
      max: this.options.max ?? 1,
      step: this.options.step ?? 0.1,
      value: this.options.value ?? 0,
    });
    if (this.options.orientation !== undefined) payload.orientation = this.options.orientation;
    if (this.options.draw_value !== undefined) payload.draw_value = this.options.draw_value;
    return { type: "slider", data: payload };
  }
}

export class Checkbox extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      id: string;
      label?: string;
      active?: boolean;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ id: this.options.id, active: this.options.active ?? false });
    if (this.options.label !== undefined) payload.label = this.options.label;
    return { type: "checkbox", data: payload };
  }
}

export class Select extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      id: string;
      items?: Array<{ id: string; label: string }>;
      selected?: number;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      id: this.options.id,
      items: this.options.items ?? [],
    });
    if (this.options.selected !== undefined) payload.selected = this.options.selected;
    return { type: "select", data: payload };
  }
}

export class Separator extends WidgetBase {
  constructor(private readonly options: CommonProps & { orientation?: Orientation } = {}) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({});
    if (this.options.orientation !== undefined) payload.orientation = this.options.orientation;
    return { type: "separator", data: payload };
  }
}

export class Scroll extends WidgetBase {
  constructor(
    private readonly child: TreeNode,
    options: CommonProps = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return { type: "scroll", data: this.withCommon({ child: this.child.toProtocol() }) };
  }
}

export class GridChild {
  constructor(
    public readonly row: number,
    public readonly column: number,
    public readonly child: TreeNode,
    public readonly width: number = 1,
    public readonly height: number = 1,
  ) {}

  toProtocol(): Record<string, unknown> {
    return {
      row: this.row,
      column: this.column,
      width: this.width,
      height: this.height,
      child: this.child.toProtocol(),
    };
  }
}

export class Grid extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      children?: GridChild[];
      row_spacing?: number;
      column_spacing?: number;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "grid",
      data: this.withCommon({
        row_spacing: this.options.row_spacing ?? 4,
        column_spacing: this.options.column_spacing ?? 4,
        children: (this.options.children ?? []).map((child) => child.toProtocol()),
      }),
    };
  }
}

export class Hero extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      title: string;
      subtitle: string;
      icon?: string;
      id?: string;
      switch?: boolean;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      title: this.options.title,
      subtitle: this.options.subtitle,
    });
    if (this.options.icon !== undefined) payload.icon = this.options.icon;
    if (this.options.id !== undefined) payload.id = this.options.id;
    if (this.options.switch !== undefined) payload.switch = this.options.switch;
    return { type: "hero", data: payload };
  }
}

export class Card extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      child?: TreeNode;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const data: Record<string, unknown> = {};
    if (this.options.child !== undefined) {
      data.child = this.options.child.toProtocol();
    }
    return {
      type: "card",
      data: this.withCommon(data),
    };
  }
}

export class Container extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      child?: TreeNode;
      width?: number;
      height?: number;
      min_width?: number;
      min_height?: number;
      margin?: Space;
      margin_top?: Space;
      margin_right?: Space;
      margin_bottom?: Space;
      margin_left?: Space;
      padding?: Space;
      padding_top?: Space;
      padding_right?: Space;
      padding_bottom?: Space;
      padding_left?: Space;
      background?: Color;
      color?: Color;
      border_radius?: Radius;
      border_width?: BorderWidth;
      border_color?: Color;
      font_size?: FontSize;
      font_weight?: FontWeight;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const data: Record<string, unknown> = {};
    if (this.options.child !== undefined) data.child = this.options.child.toProtocol();
    if (this.options.width !== undefined) data.width = this.options.width;
    if (this.options.height !== undefined) data.height = this.options.height;
    if (this.options.min_width !== undefined) data.min_width = this.options.min_width;
    if (this.options.min_height !== undefined) data.min_height = this.options.min_height;
    if (this.options.margin !== undefined) data.margin = this.options.margin;
    if (this.options.margin_top !== undefined) data.margin_top = this.options.margin_top;
    if (this.options.margin_right !== undefined) data.margin_right = this.options.margin_right;
    if (this.options.margin_bottom !== undefined) data.margin_bottom = this.options.margin_bottom;
    if (this.options.margin_left !== undefined) data.margin_left = this.options.margin_left;
    if (this.options.padding !== undefined) data.padding = this.options.padding;
    if (this.options.padding_top !== undefined) data.padding_top = this.options.padding_top;
    if (this.options.padding_right !== undefined) data.padding_right = this.options.padding_right;
    if (this.options.padding_bottom !== undefined) data.padding_bottom = this.options.padding_bottom;
    if (this.options.padding_left !== undefined) data.padding_left = this.options.padding_left;
    if (this.options.background !== undefined) data.background = this.options.background;
    if (this.options.color !== undefined) data.color = this.options.color;
    if (this.options.border_radius !== undefined) data.border_radius = this.options.border_radius;
    if (this.options.border_width !== undefined) data.border_width = this.options.border_width;
    if (this.options.border_color !== undefined) data.border_color = this.options.border_color;
    if (this.options.font_size !== undefined) data.font_size = this.options.font_size;
    if (this.options.font_weight !== undefined) data.font_weight = this.options.font_weight;
    return { type: "container", data: this.withCommon(data) };
  }
}

export class Meter extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      id?: string;
      icon?: string;
      label?: string;
      value: number;
      min?: number;
      max?: number;
      step?: number;
      text?: string;
      interactive?: boolean;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      label: this.options.label ?? "",
      value: this.options.value,
      min: this.options.min ?? 0,
      max: this.options.max ?? 1,
      step: this.options.step ?? 0.01,
      interactive: this.options.interactive ?? false,
    });
    if (this.options.id !== undefined) payload.id = this.options.id;
    if (this.options.icon !== undefined) payload.icon = this.options.icon;
    if (this.options.text !== undefined) payload.text = this.options.text;
    return { type: "meter", data: payload };
  }
}

export class Copyable extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      label?: string;
      value: string;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "copyable",
      data: this.withCommon({
        label: this.options.label ?? "",
        value: this.options.value,
      }),
    };
  }
}

export class Row extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      spacing?: number;
      children?: TreeNode[];
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "row",
      data: this.withCommon({
        spacing: this.options.spacing ?? 4,
        children: (this.options.children ?? []).map((child) => child.toProtocol()),
      }),
    };
  }
}

export class Column extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      spacing?: number;
      children?: TreeNode[];
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "column",
      data: this.withCommon({
        spacing: this.options.spacing ?? 4,
        children: (this.options.children ?? []).map((child) => child.toProtocol()),
      }),
    };
  }
}

export class Spinner extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      spinning?: boolean;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "spinner",
      data: this.withCommon({ spinning: this.options.spinning ?? true }),
    };
  }
}

export type Properties = Record<string, string>;

export class PropertyList extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      title?: string;
      rows?: Properties;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload: Record<string, unknown> = {
      rows: Object.entries(this.options.rows ?? {}).map(([key, value]) => ({ key, value })),
    };
    if (this.options.title !== undefined && this.options.title !== "") {
      payload.title = this.options.title;
    }
    return {
      type: "property_list",
      data: this.withCommon(payload),
    };
  }
}

export class Item extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      label: string;
      sublabel?: string;
      icon?: string;
      left?: TreeNode;
      right?: TreeNode;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload: Record<string, unknown> = {
      label: this.options.label,
    };
    const left =
      this.options.left ??
      (this.options.icon ? new Icon(this.options.icon, { pixel_size: 16 }) : undefined);
    if (left !== undefined) {
      payload.left = left.toProtocol();
    }
    if (this.options.sublabel !== undefined && this.options.sublabel !== "") {
      payload.sublabel = this.options.sublabel;
    }
    if (this.options.right !== undefined) {
      payload.right = this.options.right.toProtocol();
    }
    return {
      type: "item",
      data: this.withCommon(payload),
    };
  }
}

export class ActionItem extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      id: string;
      label: string;
      sublabel?: string;
      icon?: string;
      left?: TreeNode;
      right?: TreeNode;
      enabled?: boolean;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload: Record<string, unknown> = {
      id: this.options.id,
      label: this.options.label,
    };
    const left =
      this.options.left ??
      (this.options.icon ? new Icon(this.options.icon, { pixel_size: 16 }) : undefined);
    if (left !== undefined) {
      payload.left = left.toProtocol();
    }
    if (this.options.sublabel !== undefined && this.options.sublabel !== "") {
      payload.sublabel = this.options.sublabel;
    }
    if (this.options.right !== undefined) {
      payload.right = this.options.right.toProtocol();
    }
    if (this.options.enabled !== undefined) {
      payload.enabled = this.options.enabled;
    }
    return {
      type: "action_item",
      data: this.withCommon(payload),
    };
  }
}

export class EmptyState extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      title: string;
      subtitle?: string;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "empty_state",
      data: this.withCommon({
        title: this.options.title,
        subtitle: this.options.subtitle ?? "",
      }),
    };
  }
}

export class Badge extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      label: string;
      variant?: Variant;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ label: this.options.label });
    if (this.options.variant !== undefined) payload.variant = this.options.variant;
    return { type: "badge", data: payload };
  }
}

export class StatusDot extends WidgetBase {
  constructor(private readonly options: CommonProps & { variant?: StatusVariant } = {}) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({});
    if (this.options.variant !== undefined) payload.variant = this.options.variant;
    return { type: "status", data: payload };
  }
}

export class PagerItem extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      id?: string;
      appearance?: PagerAppearance;
      label?: string;
      active?: boolean;
      inactive?: boolean;
      occupied?: boolean;
      urgent?: boolean;
    } = {},
  ) {
    super(options);
  }

  toData(): Record<string, unknown> {
    const payload = this.withCommon({
      appearance: this.options.appearance ?? "dots",
      label: this.options.label ?? "",
      active: this.options.active ?? false,
      inactive: this.options.inactive ?? false,
      occupied: this.options.occupied ?? false,
      urgent: this.options.urgent ?? false,
    });
    if (this.options.id !== undefined) payload.id = this.options.id;
    return payload;
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "pager_item",
      data: this.toData(),
    };
  }
}

export class PagerStrip extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      id?: string;
      items?: PagerItem[];
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ items: (this.options.items ?? []).map((item) => item.toData()) });
    if (this.options.id) payload.id = this.options.id;
    return { type: "pager_strip", data: payload };
  }
}

export type TreeNode =
  | Hero
  | Card
  | Container
  | Meter
  | Copyable
  | PropertyList
  | Item
  | ActionItem
  | EmptyState
  | Badge
  | StatusDot
  | PagerItem
  | PagerStrip
  | Row
  | Column
  | Grid
  | Scroll
  | LevelBar
  | Progress
  | Separator
  | Spinner
  | Text
  | Icon
  | Picture
  | Button
  | LinkButton
  | Expander
  | Switch
  | ToggleButton
  | Slider
  | Select
  | Checkbox
  | PopoverScaffold;

export type PopoverSize = "small" | "medium" | "large" | "xlarge";

export class PopoverScaffold extends WidgetBase {
  constructor(
    private readonly options: {
      body: TreeNode;
      hero?: TreeNode;
      size?: PopoverSize;
    },
  ) {
    super({});
  }

  toProtocol(): Record<string, unknown> {
    const data: Record<string, unknown> = {
      size: this.options.size ?? "medium",
      body: this.options.body.toProtocol(),
    };
    if (this.options.hero !== undefined) {
      data.hero = this.options.hero.toProtocol();
    }
    return { type: "popover_scaffold", data };
  }
}
