export class StatusItem {
  constructor(
    public readonly options: {
      id?: string;
      icon?: string;
      label?: string;
      tooltip?: string;
    } = {},
  ) {}

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
    return payload;
  }
}
