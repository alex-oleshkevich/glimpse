import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  ActionItem,
  Applet,
  Badge,
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
  Progress,
  PropertyList,
  Row,
  Scroll,
  Section,
  Select,
  Separator,
  Slider,
  Spinner,
  StatusDot,
  StatusItem,
  Switch,
  ToggleButton,
  TreeExpander,
  type CallbackEvent,
  type ChangeEvent,
  type ClickEvent,
  type InputEvent,
  type ScrollEvent,
  type ToggleEvent,
  type TreeNode,
} from "glimpse-sdk";

type DemoState = {
  vpn: boolean;
  quiet: boolean;
  backup: boolean;
  brightness: number;
  cpu: number;
  profile: number;
  page: number;
  filter: string;
  syncs: number;
  popoverOpen: boolean;
  lastEvent: string;
};

const profiles = ["Balanced", "Focus", "Presentation"];
const demoPicturePath = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../assets/workstation-picture.svg",
);

class WorkstationApplet extends Applet<DemoState> {
  constructor() {
    super();
    this.onClick("sync-now", async (event) => this.syncNow(event));
    this.onClick("quiet", async () => this.setState({ quiet: !this.state.quiet, lastEvent: "quiet mode toggled" }));
    this.onClick("danger", async () => this.setState({ lastEvent: "destructive action blocked in demo" }));
    this.onClick("open-terminal", async () => this.setState({ lastEvent: "terminal shortcut selected" }));
    this.onToggle("vpn-toggle", async (event) => this.toggleBool("vpn", event));
    this.onToggle("backup-toggle", async (event) => this.toggleBool("backup", event));
    this.onToggle("focus-toggle", async (event) => this.toggleBool("quiet", event));
    this.onChange("brightness", async (event) => this.changeNumber("brightness", event));
    this.onChange("cpu-meter", async (event) => this.changeNumber("cpu", event));
    this.onChange("profile", async (event) => this.changeProfile(event));
    this.onInput("filter", async (event) => this.setState({ filter: event.text, lastEvent: `filter: ${event.text}` }));
    this.onScroll("workspace-strip", async (event) => this.rotatePage(event));
  }

  protected initialState(): DemoState {
    return {
      vpn: true,
      quiet: false,
      backup: true,
      brightness: 0.68,
      cpu: 0.42,
      profile: 0,
      page: 1,
      filter: "",
      syncs: 3,
      popoverOpen: false,
      lastEvent: "ready",
    };
  }

  protected async onCallback(event: CallbackEvent): Promise<void> {
    if (event.event === "open" || event.event === "close") {
      await this.setState({ popoverOpen: event.event === "open", lastEvent: `popover ${event.event}` });
    }
  }

  protected async status(state: DemoState): Promise<StatusItem[]> {
    const icon = state.vpn ? "network-vpn-symbolic" : "network-offline-symbolic";
    return [
      new StatusItem({ id: "workstation", icon, label: profiles[state.profile], tooltip: state.lastEvent }),
    ];
  }

  protected async popover(state: DemoState): Promise<TreeNode> {
    return new Column({
      spacing: 10,
      children: [
        new Hero({
          title: "Workstation",
          subtitle: state.popoverOpen ? "Controls are live" : "Popover is closing",
          icon: "computer-symbolic",
        }),
        new PagerStrip({
          id: "workspace-strip",
          tooltip: "Scroll to switch pages",
          items: [1, 2, 3].map((page) =>
            new PagerItem({
              appearance: "numbers",
              label: String(page),
              active: state.page === page,
              occupied: page < 3,
              urgent: page === 3 && state.cpu > 0.8,
            }),
          ),
        }),
        new Grid({
          row_spacing: 8,
          column_spacing: 8,
          children: [
            new GridChild(0, 0, metricCard("CPU", `${Math.round(state.cpu * 100)}%`, "view-statistics-symbolic")),
            new GridChild(0, 1, metricCard("Brightness", `${Math.round(state.brightness * 100)}%`, "display-brightness-symbolic")),
            new GridChild(1, 0, metricCard("Syncs", String(state.syncs), "view-refresh-symbolic")),
            new GridChild(1, 1, new StatusDot({ variant: state.vpn ? "success" : "warning" })),
          ],
        }),
        new Section({
          title: "Controls",
          subtitle: "Daily workstation settings",
          child: new Column({
            children: [
              new Row({
                spacing: 8,
                children: [
                  new Button({ id: "sync-now", label: "Sync", icon: "view-refresh-symbolic", variant: "primary" }),
                  new Button({ id: "quiet", label: state.quiet ? "Quiet" : "Focus", icon: "notifications-disabled-symbolic", variant: "secondary" }),
                  new Button({ id: "danger", label: "Reset", icon: "edit-delete-symbolic", variant: "danger", enabled: false }),
                ],
              }),
              new Switch({ id: "vpn-toggle", label: "VPN tunnel", active: state.vpn }),
              new ToggleButton({ id: "focus-toggle", label: "Focus mode", active: state.quiet }),
              new Checkbox({ id: "backup-toggle", label: "Nightly backups", active: state.backup }),
              new Slider({ id: "brightness", min: 0, max: 1, step: 0.05, value: state.brightness, draw_value: true }),
              new Meter({
                id: "cpu-meter",
                icon: "utilities-system-monitor-symbolic",
                label: "CPU pressure",
                value: state.cpu,
                max: 1,
                step: 0.01,
                text: `${Math.round(state.cpu * 100)}%`,
                interactive: true,
              }),
              new LevelBar({ value: state.cpu, min: 0, max: 1, mode: "continuous" }),
              new MenuButton({
                label: "Menu",
                icon: "open-menu-symbolic",
                popover: new Column({
                  spacing: 4,
                  children: [new Label("Quick actions"), new Badge({ label: "rendered" })],
                }),
              }),
              new Select({
                id: "profile",
                items: profiles.map((label, index) => ({ id: String(index), label })),
                selected: state.profile,
              }),
            ],
          }),
        }),
        new Section({
          title: "Queue",
          child: new Column({
            children: [
              new ActionItem({
                id: "open-terminal",
                icon: "utilities-terminal-symbolic",
                label: "Terminal session",
                sublabel: state.vpn ? "Secure session" : "Offline",
                right: new Button({ id: "open-terminal-indicator", icon: "utilities-terminal-symbolic", variant: "flat" }),
              }),
              new ListBox({
                children: [
                  new Row({ spacing: 8, children: [new Label("Build cache"), new Badge({ label: "running" })] }),
                  new Row({ spacing: 8, children: [new Label("Backup job"), new Badge({ label: "scheduled" })] }),
                ],
              }),
              new TreeExpander({
                child: new Label("Nested queue row"),
                hide_expander: true,
                indent_for_depth: true,
                indent_for_icon: true,
              }),
              new Section({
                title: "Background jobs",
                subtitle: "Build, backup, and indexing",
                child: new Column({
                  children: [
                    new Row({
                      spacing: 8,
                      children: [
                        new Label("Index packages"),
                        new Label("Index packages", { wrap: true }),
                      ],
                    }),
                    new Row({
                      spacing: 8,
                      children: [
                        new Label("Backup window"),
                        new Label(state.backup ? "02:00" : "Paused", { variant: "muted" }),
                      ],
                    }),
                  ],
                }),
              }),
            ],
          }),
        }),
        new Card({
          child: new Column({
            children: [
              new Row({
                spacing: 8,
                children: [
                  new Spinner({ spinning: state.syncs % 2 === 0 }),
                  new Icon("dialog-information-symbolic", { pixel_size: 20 }),
                  new Label("Filter input is handled through input callbacks.", { wrap: true }),
                ],
              }),
              new Copyable({ label: "Host", value: "devbox.local" }),
              new LinkButton({ uri: "https://example.com/docs", label: "Docs" }),
              new Expander({
                label: "Session details",
                expanded: state.popoverOpen,
                child: new Column({
                  spacing: 4,
                  children: [
                    new Label(`Profile: ${profiles[state.profile]}`),
                    new Label(`Last event: ${state.lastEvent}`),
                  ],
                }),
              }),
              new Overlay({
                child: new Picture({ path: demoPicturePath, content_fit: "cover" }),
                overlays: [new Badge({ label: "Live", variant: "success" })],
              }),
              new PropertyList({
                title: "Session",
                rows: {
                  Profile: profiles[state.profile],
                  "Last event": state.lastEvent,
                  Filter: state.filter || "none",
                },
              }),
            ],
          }),
        }),
        state.filter
          ? new Scroll(
              new Column({
                spacing: 4,
                children: [new Label("Recent activity", { variant: "muted" }), new Label("VPN checked"), new Label("Backups scheduled")],
              }),
            )
          : new EmptyState({ title: "No filtered activity", subtitle: "Type in the shell-provided input callback to populate this area." }),
        new Separator({ orientation: "horizontal" }),
        new Row({
          spacing: 6,
          children: [new Badge({ label: "SDK" }), new Label("All components covered", { variant: "muted" })],
        }),
      ],
    });
  }

  private async syncNow(_event: ClickEvent): Promise<void> {
    await this.setState({ syncs: this.state.syncs + 1, lastEvent: "manual sync requested" });
  }

  private async toggleBool(key: "vpn" | "backup" | "quiet", event: ToggleEvent): Promise<void> {
    await this.setState({ [key]: event.value, lastEvent: `${key}: ${event.value}` } as Partial<DemoState>);
  }

  private async changeNumber(key: "brightness" | "cpu", event: ChangeEvent): Promise<void> {
    const value = Number(event.value ?? this.state[key]);
    await this.setState({ [key]: Number.isFinite(value) ? value : this.state[key], lastEvent: `${key} changed` } as Partial<DemoState>);
  }

  private async changeProfile(event: ChangeEvent): Promise<void> {
    const raw = event.value as { index?: number } | number | undefined;
    const value = typeof raw === "object" ? raw?.index ?? 0 : raw ?? 0;
    await this.setState({ profile: Math.max(0, Math.min(profiles.length - 1, Number(value))), lastEvent: "profile changed" });
  }

  private async rotatePage(event: ScrollEvent): Promise<void> {
    const direction = (event.delta_y ?? 0) > 0 ? 1 : -1;
    const page = ((this.state.page + direction + 1) % 3) + 1;
    await this.setState({ page, lastEvent: `workspace ${page}` });
  }
}

function metricCard(label: string, value: string, icon: string): TreeNode {
  return new Card({
    child: new Column({
      children: [
        new Row({
          spacing: 6,
          children: [new Icon(icon, { pixel_size: 18 }), new Label(label)],
        }),
        new Progress({ value: Number.parseFloat(value) / 100 || 0.5, max: 1, text: value, show_text: true }),
      ],
    }),
  });
}

await new WorkstationApplet().run();
