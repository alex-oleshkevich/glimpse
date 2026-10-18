#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.14"
# dependencies = ["glimpse-applet-sdk"]
#
# [tool.uv.sources]
# glimpse-applet-sdk = { path = "../..", editable = true }
# ///
import asyncio
import dataclasses
import functools

from glimpse_sdk import (
    ActionRow,
    Addon,
    Bars,
    BaseApplet,
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
    State,
    Swatch,
    Switch,
    ToggleButton,
    ToggleRow,
    W,
)


@dataclasses.dataclass(frozen=True)
class Item:
    id: str
    title: str
    done: bool


@dataclasses.dataclass(frozen=True)
class KitchenState(State):
    count: int = 0
    seconds: int = 0
    wifi: bool = True
    remember: bool = False
    volume: float = 0.5
    scale: float = 30.0
    draft: str = ""
    choice: int = 0
    drop: int = 1
    date: str = "2026-10-15"
    month: tuple[int, int] = (2026, 10)
    expanded: bool = False
    revealed: bool = False
    page: int = 0
    wide: bool = False
    last_event: str = "none"
    items: tuple[Item, ...] = (
        Item("a", "Alpha", False),
        Item("b", "Bravo", True),
        Item("c", "Charlie", False),
    )


class Kitchen(BaseApplet[KitchenState]):
    state = KitchenState()
    persist = ("count",)

    async def start(self, ctx: Context) -> None:
        ctx.spawn(self.tick())

    async def tick(self) -> None:
        while True:
            await asyncio.sleep(1)
            self.set_state(seconds=self.state.seconds + 1)

    def event(self, name: str) -> None:
        self.set_state(last_event=name)

    def rotate(self) -> None:
        first, *rest = self.state.items
        self.set_state(items=(*rest, first), last_event="rotate")

    def increment(self) -> None:
        self.set_state(count=self.state.count + 1, last_event="increment")

    def toggle_item(self, item_id: str, done: bool) -> None:
        self.set_state(
            items=tuple(
                dataclasses.replace(item, done=done) if item.id == item_id else item
                for item in self.state.items
            )
        )

    def build(self, ctx: Context) -> list[W]:
        title = ctx.options.get("title") or "Kitchen sink"
        placement = ctx.placement or {}
        open_items = sum(not item.done for item in self.state.items)
        state = "open" if ctx.popover_open else "closed"
        zone = placement.get("zone", "?")
        history = [((self.state.seconds + i * 7) % 10) / 10 for i in range(24)]

        return [
            Indicator(
                icon="applications-graphics-symbolic",
                tooltip=title,
                badge=str(open_items),
                dot="#e01b24" if self.state.count % 2 else None,
                severity="warning" if self.state.count > 5 else None,
                attention=self.state.count > 10,
                on_press=lambda button: self.event(f"press {button}"),
                on_scroll=lambda dx, dy: self.event(f"scroll {dx:.1f} {dy:.1f}"),
                children=["KS"] if placement.get("orientation") != "vertical" else [],
            ),
            Popover(
                icon="applications-graphics-symbolic",
                title=title,
                subtitle=f"{self.state.seconds}s uptime, popover {state}",
                size="wide" if self.state.wide else "regular",
                children=[
                    Addon(
                        children=[
                            Switch(
                                active=self.state.wide,
                                on_toggle=lambda v: self.set_state(wide=v),
                            )
                        ]
                    ),
                    Notice(
                        icon="dialog-information-symbolic",
                        title="Last event",
                        subtitle=self.state.last_event,
                        severity="info",
                        on_activate=lambda: self.event("notice"),
                    ),
                    Section(
                        title="State and timers",
                        count=str(self.state.count),
                        children=[
                            Row(
                                icon="timer-symbolic",
                                title="Seconds",
                                value=str(self.state.seconds),
                                children=[Readout(value=str(self.state.seconds), unit="s")],
                            ),
                            ActionRow(
                                icon="list-add-symbolic",
                                title="Increment persisted counter",
                                subtitle="Saved to the state dir",
                                value=str(self.state.count),
                                on_activate=self.increment,
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
                                active=self.state.wifi,
                                on_toggle=lambda v: self.set_state(wifi=v),
                            ),
                            ToggleRow(
                                title="Remember",
                                control="check",
                                active=self.state.remember,
                                on_toggle=lambda v: self.set_state(remember=v),
                            ),
                            SliderRow(
                                title="Volume",
                                value=self.state.volume,
                                min=0,
                                max=1,
                                step=0.05,
                                on_move=lambda v: self.event(f"move {v:.2f}"),
                                on_change=lambda v: self.set_state(volume=v),
                                children=[
                                    Start(
                                        children=[
                                            Button(
                                                icon="audio-volume-medium-symbolic",
                                                class_name="flat",
                                                on_click=lambda: self.event("mute"),
                                            )
                                        ]
                                    )
                                ],
                            ),
                            Scale(
                                value=self.state.scale,
                                min=0,
                                max=100,
                                step=1,
                                on_change=lambda v: self.set_state(scale=v),
                            ),
                            Entry(
                                placeholder="Type, then Enter",
                                value=self.state.draft,
                                on_change=lambda v: self.set_state(draft=v),
                                on_submit=lambda v: self.event(f"submit {v}"),
                            ),
                            DropDown(
                                options=["Small", "Medium", "Large"],
                                selected=self.state.drop,
                                on_select=lambda i: self.set_state(drop=i),
                            ),
                            ChoiceList(
                                choices=[
                                    {"label": "First", "detail": "detail one"},
                                    {"label": "Second", "icon": "emblem-ok-symbolic"},
                                    {"label": "Third", "warning": True},
                                ],
                                selected=self.state.choice,
                                on_select=lambda i: self.set_state(choice=i),
                            ),
                            Box(
                                orientation="horizontal",
                                spacing=6,
                                children=[
                                    ToggleButton(
                                        label="Toggle",
                                        active=self.state.wifi,
                                        on_toggle=lambda v: self.set_state(wifi=v),
                                    ),
                                    CheckButton(
                                        label="Check",
                                        active=self.state.remember,
                                        on_toggle=lambda v: self.set_state(remember=v),
                                    ),
                                    Button(text="Plain", on_click=lambda: self.event("button")),
                                ],
                            ),
                            Calendar(
                                year=self.state.month[0],
                                month=self.state.month[1],
                                selected=self.state.date,
                                today="2026-10-15",
                                events=[{"date": "2026-10-15", "colors": ["accent"]}],
                                on_month=lambda y, m: self.set_state(month=(y, m)),
                                on_select=lambda d: self.set_state(date=d),
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
                                    on_toggle=functools.partial(self.toggle_item, item.id),
                                )
                                for item in self.state.items
                            ],
                            ActionRow(
                                title="Rotate list",
                                subtitle="Keyed rows move, state follows",
                                on_activate=self.rotate,
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
                            RangeBar(min=0, max=100, low=20, high=80, now=self.state.seconds % 100),
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
                                    {"label": "Counter", "value": str(self.state.count)},
                                    {"label": "Date", "value": self.state.date},
                                    {
                                        "label": "Draft",
                                        "value": self.state.draft or "-",
                                        "warning": bool(self.state.draft),
                                    },
                                ]
                            ),
                            ActionRow(
                                title="Event dots",
                                on_activate=lambda: self.event("dots"),
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
                                on_activate=lambda: self.event("swatch"),
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
                                active=self.state.revealed,
                                on_toggle=lambda v: self.set_state(revealed=v),
                            ),
                            Revealer(
                                revealed=self.state.revealed,
                                transition="slide-down",
                                children=[
                                    Row(title="Revealed row", subtitle="Inside a revealer"),
                                ],
                            ),
                            ActionRow(
                                title="Next stack page",
                                value=str(self.state.page),
                                on_activate=lambda: self.set_state(page=(self.state.page + 1) % 3),
                            ),
                            Stack(
                                visible=self.state.page,
                                children=[
                                    Label(text="Page one"),
                                    Label(text="Page two"),
                                    Label(text="Page three"),
                                ],
                            ),
                            ExpanderRow(
                                icon="bluetooth-symbolic",
                                title="Expander",
                                open=self.state.expanded,
                                on_toggle=lambda v: self.set_state(expanded=v),
                                children=[
                                    End(children=[Image(icon="object-select-symbolic")]),
                                    ActionRow(
                                        title="Drawer action",
                                        on_activate=lambda: self.event("drawer"),
                                    ),
                                    MeterRow(title="Battery", level=0.6, value="60%"),
                                    ToggleRow(
                                        title="Drawer toggle",
                                        active=self.state.wifi,
                                        on_toggle=lambda v: self.set_state(wifi=v),
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
                                on_activate=lambda: ctx.copy(self.state.draft or "kitchen sink"),
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
                ],
            ),
        ]


if __name__ == "__main__":
    Kitchen.main()
