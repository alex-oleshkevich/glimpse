"""Golden cross-SDK fixture tests."""

from __future__ import annotations

import json
import unittest
from dataclasses import asdict
from pathlib import Path

from glimpse_sdk import (
    Badge,
    BadgeKind,
    BatteryHero,
    BoxedList,
    ButtonRow,
    Calendar,
    CameraIndicator,
    Choice,
    ChoiceList,
    ChoiceTile,
    Column,
    Container,
    ContainerBg,
    DateHero,
    EmptyState,
    EventItem,
    Events,
    ExpanderTile,
    FontSize,
    FontWeight,
    Header,
    Hero,
    KeyValueGrid,
    LocationIndicator,
    Meter,
    MicIndicator,
    MutedIndicator,
    PagerAppearance,
    PagerItem,
    PagerStrip,
    PanelIndicator,
    PopoverShell,
    PopoverSize,
    Radius,
    Row,
    ScreenCastIndicator,
    Scroll,
    SegmentedTile,
    Separator,
    SliderTile,
    Space,
    Spinner,
    StatusDot,
    StatusDotStatus,
    SwitchTile,
    Text,
    TextColor,
    Tile,
    WeatherForecastItem,
    WeatherForecastList,
    WeatherHourlyItem,
    WeatherHourlyStrip,
    WorldClock,
    WorldClockRow,
)
from glimpse_sdk.events import parse_callback_event

FIXTURES = Path(__file__).resolve().parents[2] / "fixtures"


def load(rel: str) -> dict:
    return json.loads((FIXTURES / rel).read_text())


def widgets() -> dict[str, object]:
    text = Text(text="Ready", size=FontSize.SM, weight=FontWeight.MEDIUM, color=TextColor.MUTED, wrap=True)
    badge = Badge(label="OK", kind=BadgeKind.SUCCESS)
    status = StatusDot(status=StatusDotStatus.WARNING)
    return {
        "text": text,
        "header": Header(label="Network"),
        "hero": Hero(
            id="vpn",
            icon="network-vpn-symbolic",
            icon_size=32,
            title="VPN",
            subtitle="Disconnected",
            toggle=False,
            toggle_sensitive=True,
            separator=True,
            trailing=badge,
        ),
        "badge": badge,
        "status-dot": status,
        "panel-indicator": PanelIndicator(
            id="net",
            icon="network-wireless-symbolic",
            label="Wi-Fi",
            active=True,
            extra=status,
        ),
        "empty-state": EmptyState(title="No devices", subtitle="Connect a device to continue"),
        "spinner": Spinner(),
        "meter": Meter(label="Memory", value=0.51),
        "separator": Separator(),
        "scroll": Scroll(child=text),
        "row": Row(children=[text, badge]),
        "column": Column(children=[text, badge]),
        "container": Container(
            children=[text],
            padding=Space.S4,
            margin=Space.S2,
            radius=Radius.MD,
            bg=ContainerBg.SURFACE,
            border_width=1,
            min_width=Space.S8,
            min_height=Space.S4,
        ),
        "boxed-list": BoxedList(children=[text, badge]),
        "popover-shell": PopoverShell(size=PopoverSize.MEDIUM, children=[text], footer=[badge], footer_visible=True),
        "tile": Tile(
            id="wifi",
            primary="Wi-Fi",
            secondary="Connected",
            left_icon="network-wireless-symbolic",
            right=badge,
            activatable=True,
        ),
        "segmented-tile": SegmentedTile(
            id="drive",
            primary="Backup",
            secondary="Mounted",
            left_icon="drive-harddisk-symbolic",
            right=badge,
            child=KeyValueGrid(rows=[("Size", "1 TB")]),
            expanded=True,
            activatable=True,
        ),
        "button-row": ButtonRow(children=[Tile(primary="Refresh", activatable=True)]),
        "switch-tile": SwitchTile(
            id="bluetooth",
            primary="Bluetooth",
            secondary="On",
            left_icon="bluetooth-active-symbolic",
            active=True,
        ),
        "expander-tile": ExpanderTile(
            id="details",
            primary="Details",
            secondary="2 items",
            left_icon="view-list-symbolic",
            child=Column(children=[text]),
            expanded=True,
        ),
        "slider-tile": SliderTile(
            id="brightness",
            label="Brightness",
            left_icon="display-brightness-symbolic",
            value=0.6,
            min=0.0,
            max=1.0,
            step=0.05,
            page=0.1,
            digits=0,
            snap_step=0.05,
        ),
        "choice-tile": ChoiceTile(
            id="balanced",
            primary="Balanced",
            secondary="Recommended",
            left_icon="power-profile-balanced-symbolic",
            selected=True,
        ),
        "choice-list": ChoiceList(
            id="profile",
            active="balanced",
            choices=[
                Choice("balanced", "Balanced", "Recommended", "power-profile-balanced-symbolic"),
                Choice("performance", "Performance", "Fast", "power-profile-performance-symbolic"),
            ],
        ),
        "key-value-grid": KeyValueGrid(rows=[("IPv4", "10.0.0.42")]),
        "pager-item": PagerItem(
            id=1,
            label="1",
            appearance=PagerAppearance.NUMBERS,
            active=True,
            occupied=True,
        ),
        "pager-strip": PagerStrip(
            id="workspaces",
            items=[
                PagerItem(id=1, label="1", appearance=PagerAppearance.NUMBERS, active=True, occupied=True),
                PagerItem(id=2, label="2", appearance=PagerAppearance.NUMBERS, inactive=True),
            ],
        ),
        "camera-indicator": CameraIndicator(active=True),
        "mic-indicator": MicIndicator(active=True),
        "muted-indicator": MutedIndicator(active=True),
        "screencast-indicator": ScreenCastIndicator(active=True, timer_text="01:23"),
        "location-indicator": LocationIndicator(active=True),
        "calendar": Calendar(id="calendar", selected_date="2026-05-22", event_days=["2026-05-22", "2026-05-24"]),
        "battery-hero": BatteryHero(
            icon="battery-good-symbolic",
            percentage="82%",
            fraction=0.82,
            state="Discharging",
        ),
        "date-hero": DateHero(weekday="Friday", date="May 22"),
        "events": Events(
            date="2026-05-22",
            events=[EventItem(id="standup", title="Standup", start="09:30", end="09:45")],
        ),
        "weather-forecast-list": WeatherForecastList(
            items=[
                WeatherForecastItem(
                    day_name="Today",
                    icon="weather-clear-symbolic",
                    condition="Clear",
                    temperatures="12 / 20",
                    is_today=True,
                )
            ]
        ),
        "weather-hourly-strip": WeatherHourlyStrip(
            items=[WeatherHourlyItem(time="12:00", icon="weather-clear-symbolic", temperature="18")]
        ),
        "world-clock": WorldClock(
            rows=[WorldClockRow(name="UTC", timezone="UTC", time="12:00", offset="+00:00", day_label="Today")]
        ),
        "tree-shared-popover": PopoverShell(
            size=PopoverSize.LARGE,
            children=[
                Hero(title="System", subtitle="Shared widgets"),
                BoxedList(children=[SwitchTile(id="wifi", primary="Wi-Fi", active=True)]),
            ],
        ),
    }


class GoldenWidgetTests(unittest.TestCase):
    def test_widgets_match_fixtures(self) -> None:
        for name, widget in widgets().items():
            with self.subTest(name=name):
                self.assertEqual(widget.to_protocol(), load(f"widgets/{name}.json"))


class GoldenEventTests(unittest.TestCase):
    def _fixture(self, name: str) -> tuple[dict, dict]:
        fixture = load(f"events/{name}.json")
        return fixture["incoming"], fixture["parsed"]

    def test_events_match_fixtures(self) -> None:
        for path in sorted((FIXTURES / "events").glob("*.json")):
            incoming, parsed = self._fixture(path.stem)
            with self.subTest(name=path.stem):
                event = parse_callback_event(incoming)
                self.assertEqual(asdict(event), parsed)


if __name__ == "__main__":
    unittest.main()
