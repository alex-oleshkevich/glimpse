from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from glimpse_sdk import (
    ActionItem,
    Applet,
    AppletState,
    Badge,
    Box,
    Button,
    ButtonVariant,
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
    Label,
    LevelBar,
    LinkButton,
    ListBox,
    MenuButton,
    Meter,
    Overlay,
    Orientation,
    PagerAppearance,
    PagerItem,
    PagerStrip,
    Picture,
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
    StatusItem,
    Switch,
    ToggleButton,
    TreeExpander,
    Variant,
    change,
    click,
    event,
    input as input_event,
    scroll,
    toggle,
)


PROFILES = ["Balanced", "Performance", "Battery saver"]
DEMO_PICTURE_PATH = str(Path(__file__).resolve().parents[1] / "assets/workstation-picture.svg")


@dataclass
class DemoState(AppletState):
    cpu: float = 0.42
    memory: float = 0.58
    disk: float = 0.71
    brightness: float = 65
    profile: int = 0
    vpn: bool = True
    focus: bool = False
    backups: bool = True
    page: int = 1
    filter_text: str = ""
    last_event: str = "started"
    popover_open: bool = False
    syncing: bool = False


class WorkstationDemo(Applet[DemoState]):
    def initial_state(self) -> DemoState:
        return DemoState()

    async def status(self, state: DemoState) -> list[StatusItem]:
        label = f"{int(state.cpu * 100)}% {PROFILES[state.profile]}"
        return [
            StatusItem(
                id="workstation",
                icon=Icon.name("utilities-system-monitor-symbolic"),
                label=label,
                tooltip=f"Memory {int(state.memory * 100)}%, disk {int(state.disk * 100)}%",
            ),
            StatusItem(
                id="vpn",
                icon=Icon.name("network-vpn-symbolic"),
                label="VPN" if state.vpn else "Off",
                tooltip="Secure tunnel status",
            ),
        ]

    async def popover(self, state: DemoState):
        if state.filter_text == "empty":
            return EmptyState(
                title="No matching workstation signals",
                subtitle="Clear the filter to show the demo data.",
            )

        return Column(
            spacing=10,
            children=[
                Hero(
                    title="Workstation",
                    subtitle=f"{PROFILES[state.profile]} - {state.last_event}",
                    icon=Icon.name("computer-symbolic"),
                ),
                PagerStrip(
                    id="workspace-strip",
                    items=[
                        PagerItem(appearance=PagerAppearance.NUMBERS, label="1", active=state.page == 1, occupied=True),
                        PagerItem(appearance=PagerAppearance.NUMBERS, label="2", active=state.page == 2, occupied=True),
                        PagerItem(appearance=PagerAppearance.NUMBERS, label="3", active=state.page == 3, urgent=state.cpu > 0.8),
                    ],
                ),
                Section(
                    title="Overview",
                    subtitle="Live machine shape",
                    children=[
                        Grid(
                            row_spacing=6,
                            column_spacing=8,
                            children=[
                                GridChild(0, 0, MetricCard("CPU", state.cpu, "processor-symbolic")),
                                GridChild(0, 1, MetricCard("Memory", state.memory, "drive-harddisk-symbolic")),
                                GridChild(1, 0, Progress(value=state.disk, max=1, show_text=True, text=f"{int(state.disk * 100)}% disk")),
                                GridChild(1, 1, StatusDot(variant=Variant.SUCCESS if state.vpn else Variant.WARNING)),
                            ],
                        ),
                        PropertyList(
                            title="Session",
                            rows={
                                "Host": "demo-laptop",
                                "Profile": PROFILES[state.profile],
                                "Last event": state.last_event,
                            },
                        ),
                    ],
                ),
                Section(
                    title="Controls",
                    children=[
                        Switch(id="vpn-toggle", label="VPN tunnel", active=state.vpn),
                        ToggleButton(id="focus-toggle", label="Focus mode", active=state.focus),
                        Checkbox(id="backup-toggle", label="Nightly backup", active=state.backups),
                        Slider(
                            id="brightness",
                            min=0,
                            max=100,
                            step=5,
                            value=state.brightness,
                            orientation=Orientation.HORIZONTAL,
                            draw_value=True,
                        ),
                        Select(
                            id="profile",
                            items=[SelectOption(str(index), label) for index, label in enumerate(PROFILES)],
                            selected=state.profile,
                        ),
                        Meter(
                            id="cpu-meter",
                            icon=Icon.name("processor-symbolic"),
                            label="CPU limit",
                            value=state.cpu,
                            min=0,
                            max=1,
                            step=0.05,
                            text=f"{int(state.cpu * 100)}%",
                            interactive=True,
                        ),
                        LevelBar(value=state.cpu, min=0, max=1, mode="continuous"),
                        MenuButton(
                            label="Menu",
                            icon="open-menu-symbolic",
                            popover=Column(
                                spacing=4,
                                children=[
                                    Label(text="Quick actions"),
                                    Badge(label="rendered"),
                                ],
                            ),
                        ),
                    ],
                ),
                Section(
                    title="Operations",
                    subtitle="Actions and nested rows",
                    children=[
                        ActionItem(
                            id="open-terminal",
                            icon="utilities-terminal-symbolic",
                            label="Open terminal on host",
                            sublabel="Local secure session",
                            right=Button(
                                id="open-terminal-indicator",
                                icon="utilities-terminal-symbolic",
                                variant=ButtonVariant.FLAT,
                            ),
                        ),
                        ListBox(
                            children=[
                                Row(spacing=8, children=[Label(text="Build cache"), Badge(label="running")]),
                                Row(spacing=8, children=[Label(text="Backup job"), Badge(label="scheduled")]),
                            ]
                        ),
                        TreeExpander(
                            child=Label(text="Nested queue row"),
                            hide_expander=True,
                            indent_for_depth=True,
                            indent_for_icon=True,
                        ),
                        Row(
                            spacing=8,
                            children=[
                                Spinner(spinning=state.syncing),
                                Column(
                                    spacing=2,
                                    children=[
                                        Label(text="Maintenance queue"),
                                        Label(text="Index packages", wrap=True),
                                    ],
                                ),
                                Badge(label="2 jobs"),
                            ],
                        ),
                        Copyable(label="Run ID", value="demo-2026-05-13"),
                        LinkButton(uri="https://example.com/docs", label="Docs"),
                        Expander(
                            label="Session details",
                            expanded=state.popover_open,
                            child=Column(
                                spacing=4,
                                children=[
                                    Label(text=f"Profile: {PROFILES[state.profile]}"),
                                    Label(text=f"Last event: {state.last_event}"),
                                ],
                            ),
                        ),
                        Row(
                            spacing=6,
                            children=[
                                Button(id="sync-now", label="Sync", icon="view-refresh-symbolic", variant=ButtonVariant.PRIMARY),
                                Button(id="quiet", label="Quiet", icon="notifications-disabled-symbolic", variant=ButtonVariant.FLAT),
                                Button(id="danger", label="Abort", icon="process-stop-symbolic", variant=ButtonVariant.DANGER, enabled=state.syncing),
                            ],
                        ),
                    ],
                ),
                Card(
                    children=[
                        Box.horizontal(
                            [
                                Image(icon=Icon.name("dialog-information-symbolic"), pixel_size=20),
                                Label(text="Filter is handled through input callbacks; send event id=filter type=input."),
                            ],
                            spacing=8,
                        ),
                        Overlay(
                            child=Picture(path=DEMO_PICTURE_PATH, content_fit="cover"),
                            overlays=[Badge(label="Live", variant=Variant.SUCCESS)],
                        ),
                        Scroll(
                            child=Column(
                                spacing=4,
                                children=[
                                    Label(text="Recent activity", variant=Variant.MUTED),
                                    Label(text="VPN checked"),
                                    Label(text="Backups scheduled"),
                                    Label(text="Disk pressure normal"),
                                ],
                            )
                        ),
                    ]
                ),
                Separator(orientation=Orientation.HORIZONTAL),
            ],
        )

    @click("sync-now")
    async def sync_now(self, _event) -> None:
        await self.set_state(syncing=not self.state.syncing, last_event="sync toggled")

    @click("quiet")
    async def quiet(self, _event) -> None:
        await self.set_state(focus=not self.state.focus, last_event="quiet mode")

    @click("danger")
    async def abort(self, _event) -> None:
        await self.set_state(syncing=False, last_event="operation aborted")

    @click("open-terminal")
    async def open_terminal(self, _event) -> None:
        await self.set_state(last_event="terminal requested")

    @toggle("vpn-toggle")
    async def toggle_vpn(self, event) -> None:
        await self.set_state(vpn=event.value, last_event="vpn toggled")

    @toggle("backup-toggle")
    async def toggle_backups(self, event) -> None:
        await self.set_state(backups=event.value, last_event="backup toggled")

    @toggle("focus-toggle")
    async def toggle_focus(self, event) -> None:
        await self.set_state(focus=event.value, last_event="focus toggled")

    @change("brightness")
    async def change_brightness(self, event) -> None:
        await self.set_state(brightness=float(event.value), last_event="brightness changed")

    @change("cpu-meter")
    async def change_cpu(self, event) -> None:
        await self.set_state(cpu=float(event.value), last_event="cpu limit changed")

    @change("profile")
    async def change_profile(self, event) -> None:
        value = event.value
        index = int(value.get("index", 0) if isinstance(value, dict) else value)
        await self.set_state(profile=max(0, min(index, len(PROFILES) - 1)), last_event="profile changed")

    @input_event("filter")
    async def filter_changed(self, event) -> None:
        await self.set_state(filter_text=event.text, last_event="filter changed")

    @scroll("workspace-strip")
    async def scroll_pages(self, event) -> None:
        delta = -1 if (event.delta_y or 0) < 0 else 1
        await self.set_state(page=((self.state.page + delta - 1) % 3) + 1, last_event="workspace changed")

    @event("open", "popover")
    async def popover_opened(self, _event) -> None:
        await self.set_state(popover_open=True, last_event="popover opened")

    @event("close", "popover")
    async def popover_closed(self, _event) -> None:
        await self.set_state(popover_open=False, last_event="popover closed")


def MetricCard(label: str, value: float, icon_name: str) -> Card:
    return Card(
        children=[
            Row(
                spacing=6,
                children=[
                    Image(icon=Icon.name(icon_name), pixel_size=18),
                    Label(text=label),
                    Badge(label=f"{int(value * 100)}%"),
                ],
            ),
            Progress(value=value, max=1, show_text=True, text=f"{int(value * 100)}%"),
        ]
    )


if __name__ == "__main__":
    WorkstationDemo().run()
