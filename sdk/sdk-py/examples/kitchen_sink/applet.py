#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.14"
# dependencies = ["glimpse-applet-sdk"]
#
# [tool.uv.sources]
# glimpse-applet-sdk = { path = "../..", editable = true }
# ///
import asyncio
import contextlib
import dataclasses
import functools
import json
import pathlib

from glimpse_sdk import (
    ActionRow,
    Addon,
    Bars,
    Box,
    Button,
    Calendar,
    CheckButton,
    ChoiceList,
    Container,
    Context,
    Dots,
    DropDown,
    End,
    Entry,
    ExpanderRow,
    FactList,
    Image,
    Indicator,
    Label,
    MeterRow,
    Notice,
    Placeholder,
    Popover,
    RangeBar,
    Readout,
    Revealer,
    Row,
    Scale,
    Section,
    Separator,
    SliderRow,
    Sparkline,
    Spinner,
    Stack,
    Start,
    Swatch,
    Switch,
    ToggleButton,
    ToggleRow,
    W,
    run,
)


@dataclasses.dataclass
class Item:
    id: str
    title: str
    done: bool


class App:
    def __init__(self) -> None:
        self.ctx: Context | None = None
        self.count = 0
        self.seconds = 0
        self.wifi = True
        self.remember = False
        self.volume = 0.5
        self.scale = 30.0
        self.draft = ""
        self.choice = 0
        self.drop = 1
        self.date = "2026-10-15"
        self.month = (2026, 10)
        self.expanded = False
        self.revealed = False
        self.page = 0
        self.wide = False
        self.last_event = "none"
        self.items = [
            Item("a", "Alpha", False),
            Item("b", "Bravo", True),
            Item("c", "Charlie", False),
        ]

    async def start(self, ctx: Context) -> None:
        self.ctx = ctx

        with contextlib.suppress(Exception):
            self.count = int(
                json.loads((pathlib.Path(ctx.state_dir) / "state.json").read_text())["count"]
            )

        while True:
            await asyncio.sleep(1)
            self.seconds += 1
            ctx.wake()

    def _save(self) -> None:
        if self.ctx is not None and self.ctx.state_dir:
            target = pathlib.Path(self.ctx.state_dir) / "state.json"
            target.write_text(json.dumps({"count": self.count}))

    def _event(self, name: str) -> None:
        self.last_event = name

    def build(self, ctx: Context) -> list[W]:
        title = ctx.options.get("title") or "Kitchen sink"
        placement = ctx.placement or {}
        players = ctx.mpris or []
        compositor = ctx.compositor or {}
        workspaces = compositor.get("workspaces") or []
        open_items = sum(not item.done for item in self.items)
        state = "open" if ctx.popover_open else "closed"
        zone = placement.get("zone", "?")
        history = [((self.seconds + i * 7) % 10) / 10 for i in range(24)]

        player_rows = [
            ActionRow(
                key=player["id"],
                icon="media-playback-start-symbolic",
                title=player.get("title") or player.get("identity") or "Player",
                subtitle=player.get("artist") or player.get("playback") or "",
                on_activate=functools.partial(ctx.mpris_control, player["id"], "play-pause"),
            )
            for player in players
        ] or [
            Placeholder(
                icon="media-playback-stop-symbolic",
                title="No players",
                description="MPRIS service is empty",
            )
        ]

        workspace_rows = [
            ActionRow(
                key=ws["id"],
                title=ws.get("name") or f"Workspace {ws['id']}",
                subtitle=f"{ws.get('windows', 0)} windows",
                on_activate=functools.partial(ctx.focus_workspace, {"id": ws["id"]}),
            )
            for ws in workspaces
        ] or [
            Placeholder(
                icon="window-symbolic",
                title="No workspaces",
                description="Compositor service is empty",
            )
        ]

        return [
            Indicator(
                icon="applications-graphics-symbolic",
                tooltip=title,
                badge=str(open_items),
                dot="#e01b24" if self.count % 2 else None,
                severity="warning" if self.count > 5 else None,
                attention=self.count > 10,
                on_press=lambda button: self._event(f"press {button}"),
                on_scroll=lambda dx, dy: self._event(f"scroll {dx:.1f} {dy:.1f}"),
                children=["KS"] if placement.get("orientation") != "vertical" else [],
            ),
            Popover(
                icon="applications-graphics-symbolic",
                title=title,
                subtitle=f"{self.seconds}s uptime, popover {state}",
                size="wide" if self.wide else "regular",
                children=[
                    Addon(
                        children=[
                            Switch(active=self.wide, on_toggle=lambda v: setattr(self, "wide", v))
                        ]
                    ),
                    Notice(
                        icon="dialog-information-symbolic",
                        title="Last event",
                        subtitle=self.last_event,
                        severity="info",
                        on_activate=lambda: self._event("notice"),
                    ),
                    Section(
                        title="State and timers",
                        count=str(self.count),
                        children=[
                            Row(
                                icon="timer-symbolic",
                                title="Seconds",
                                value=str(self.seconds),
                                children=[Readout(value=str(self.seconds), unit="s")],
                            ),
                            ActionRow(
                                icon="list-add-symbolic",
                                title="Increment persisted counter",
                                subtitle="Saved to the state dir",
                                value=str(self.count),
                                on_activate=self._increment,
                            ),
                            Row(title="Applet", value=ctx.name or "?"),
                            Row(
                                title="Placement",
                                value=f"{placement.get('position', '?')} {zone}",
                            ),
                            Row(
                                title="State dir",
                                value="set" if ctx.state_dir else "unset",
                                children=[Spinner()],
                            ),
                        ],
                    ),
                    Section(
                        title="Controls",
                        children=[
                            ToggleRow(
                                icon="network-wireless-symbolic",
                                title="Wi-Fi",
                                active=self.wifi,
                                on_toggle=lambda v: setattr(self, "wifi", v),
                            ),
                            ToggleRow(
                                title="Remember",
                                control="check",
                                active=self.remember,
                                on_toggle=lambda v: setattr(self, "remember", v),
                            ),
                            SliderRow(
                                title="Volume",
                                value=self.volume,
                                min=0,
                                max=1,
                                step=0.05,
                                on_move=lambda v: self._event(f"move {v:.2f}"),
                                on_change=lambda v: setattr(self, "volume", v),
                                children=[
                                    Start(
                                        children=[
                                            Button(
                                                icon="audio-volume-medium-symbolic",
                                                class_name="flat",
                                                on_click=lambda: self._event("mute"),
                                            )
                                        ]
                                    )
                                ],
                            ),
                            Scale(
                                value=self.scale,
                                min=0,
                                max=100,
                                step=1,
                                on_change=lambda v: setattr(self, "scale", v),
                            ),
                            Entry(
                                placeholder="Type, then Enter",
                                value=self.draft,
                                on_change=lambda v: setattr(self, "draft", v),
                                on_submit=lambda v: self._event(f"submit {v}"),
                            ),
                            DropDown(
                                options=["Small", "Medium", "Large"],
                                selected=self.drop,
                                on_select=lambda i: setattr(self, "drop", i),
                            ),
                            ChoiceList(
                                choices=[
                                    {"label": "First", "detail": "detail one"},
                                    {"label": "Second", "icon": "emblem-ok-symbolic"},
                                    {"label": "Third", "warning": True},
                                ],
                                selected=self.choice,
                                on_select=lambda i: setattr(self, "choice", i),
                            ),
                            Box(
                                orientation="horizontal",
                                spacing=6,
                                children=[
                                    ToggleButton(
                                        label="Toggle",
                                        active=self.wifi,
                                        on_toggle=lambda v: setattr(self, "wifi", v),
                                    ),
                                    CheckButton(
                                        label="Check",
                                        active=self.remember,
                                        on_toggle=lambda v: setattr(self, "remember", v),
                                    ),
                                    Button(text="Plain", on_click=lambda: self._event("button")),
                                ],
                            ),
                            Calendar(
                                year=self.month[0],
                                month=self.month[1],
                                selected=self.date,
                                today="2026-10-15",
                                events=[{"date": "2026-10-15", "colors": ["accent"]}],
                                on_month=lambda y, m: setattr(self, "month", (y, m)),
                                on_select=lambda d: setattr(self, "date", d),
                            ),
                        ],
                    ),
                    Section(
                        title="Keyed list",
                        count=str(open_items),
                        children=[
                            *[
                                ToggleRow(
                                    key=item.id,
                                    title=item.title,
                                    control="check",
                                    active=item.done,
                                    on_toggle=functools.partial(setattr, item, "done"),
                                )
                                for item in self.items
                            ],
                            ActionRow(
                                title="Rotate list",
                                subtitle="Keyed rows move, state follows",
                                on_activate=self._rotate,
                            ),
                        ],
                    ),
                    Section(
                        title="Data",
                        children=[
                            MeterRow(
                                title="CPU",
                                level=0.3,
                                value="30%",
                                children=[Image(icon="computer-symbolic")],
                            ),
                            MeterRow(title="Memory", level=0.8, tone="warning", value="80%"),
                            RangeBar(min=0, max=100, low=20, high=80, now=self.seconds % 100),
                            Bars(
                                values=history,
                                maximum=1,
                                highlight_last=True,
                                label="History",
                                tooltips=[f"{v:.0%}" for v in history],
                            ),
                            Sparkline(values=history, fill=True, show_last=True, label="Trend"),
                            FactList(
                                facts=[
                                    {"label": "Counter", "value": str(self.count)},
                                    {"label": "Date", "value": self.date},
                                    {
                                        "label": "Draft",
                                        "value": self.draft or "-",
                                        "warning": bool(self.draft),
                                    },
                                ]
                            ),
                            ActionRow(
                                title="Event dots",
                                on_activate=lambda: self._event("dots"),
                                children=[
                                    Start(
                                        children=[
                                            Dots(
                                                colors=["accent", "success", "warning"],
                                                size=10,
                                                max=3,
                                            )
                                        ]
                                    )
                                ],
                            ),
                            ActionRow(
                                title="Color swatch",
                                on_activate=lambda: self._event("swatch"),
                                children=[
                                    Start(children=[Swatch(color="#3584e4", tooltip="Blue")])
                                ],
                            ),
                            Container(
                                height=44,
                                background="blue",
                                color="light",
                                align="center",
                                children=[Label(text="Container with a label")],
                            ),
                            Separator(),
                            Label(text="A plain label", class_name="dim-label"),
                        ],
                    ),
                    Section(
                        title="Layout",
                        children=[
                            ToggleRow(
                                title="Reveal more",
                                active=self.revealed,
                                on_toggle=lambda v: setattr(self, "revealed", v),
                            ),
                            Revealer(
                                revealed=self.revealed,
                                transition="slide-down",
                                children=[
                                    Row(title="Revealed row", subtitle="Inside a revealer"),
                                ],
                            ),
                            ActionRow(
                                title="Next stack page",
                                value=str(self.page),
                                on_activate=lambda: setattr(self, "page", (self.page + 1) % 3),
                            ),
                            Stack(
                                visible=self.page,
                                children=[
                                    Label(text="Page one"),
                                    Label(text="Page two"),
                                    Label(text="Page three"),
                                ],
                            ),
                            ExpanderRow(
                                icon="bluetooth-symbolic",
                                title="Expander",
                                open=self.expanded,
                                on_toggle=lambda v: setattr(self, "expanded", v),
                                children=[
                                    End(children=[Image(icon="object-select-symbolic")]),
                                    ActionRow(
                                        title="Drawer action",
                                        on_activate=lambda: self._event("drawer"),
                                    ),
                                    MeterRow(title="Battery", level=0.6, value="60%"),
                                    ToggleRow(
                                        title="Drawer toggle",
                                        active=self.wifi,
                                        on_toggle=lambda v: setattr(self, "wifi", v),
                                    ),
                                ],
                            ),
                        ],
                    ),
                    Section(
                        title="Host requests",
                        children=[
                            ActionRow(
                                title="Notify",
                                on_activate=lambda: ctx.notify(
                                    "Kitchen sink",
                                    body="Hello from Python",
                                    icon="applications-graphics-symbolic",
                                ),
                            ),
                            ActionRow(
                                title="Copy draft",
                                on_activate=lambda: ctx.copy(self.draft or "kitchen sink"),
                            ),
                            ActionRow(
                                title="Open example.org",
                                on_activate=lambda: ctx.open_uri("https://example.org"),
                            ),
                            ActionRow(
                                title="Launch files",
                                on_activate=lambda: ctx.launch("org.gnome.Nautilus"),
                            ),
                            ActionRow(
                                title="Close popover",
                                tone="destructive",
                                on_activate=ctx.close_popover,
                            ),
                        ],
                    ),
                    Section(title="Players (MPRIS service)", children=player_rows),
                    Section(title="Workspaces (compositor service)", children=workspace_rows),
                ],
            ),
        ]

    def _rotate(self) -> None:
        self.items.append(self.items.pop(0))
        self._event("rotate")

    def _increment(self) -> None:
        self.count += 1
        self._save()
        self._event("increment")


run(App())
