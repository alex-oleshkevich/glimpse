export class StatusItem {
  constructor(
    public readonly options: {
      id?: string;
      icon?: string;
      label?: string;
      tooltip?: string;
      cssClasses?: string[];
    } = {},
  ) {}

  /**
   * Return a copy with `cssClass` appended to `cssClasses`. Immutable-update
   * form that mirrors the Rust SDK's `css_class()` builder so SDK-hopping
   * users see equivalent ergonomics across languages.
   */
  withCssClass(cssClass: string): StatusItem {
    return new StatusItem({
      ...this.options,
      cssClasses: [...(this.options.cssClasses ?? []), cssClass],
    });
  }

  toProtocol(): Record<string, unknown> {
    const payload: Record<string, unknown> = {};
    if (this.options.id !== undefined) {
      payload.id = this.options.id;
    }
    if (this.options.icon !== undefined) {
      payload.icon = this.options.icon;
    }
    if (this.options.label !== undefined) {
      payload.label = this.options.label;
    }
    if (this.options.tooltip !== undefined) {
      payload.tooltip = this.options.tooltip;
    }
    if (this.options.cssClasses && this.options.cssClasses.length > 0) {
      payload.css_classes = [...this.options.cssClasses];
    }
    return payload;
  }
}
