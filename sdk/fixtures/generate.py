#!/usr/bin/env python3
"""Generate the shared golden-fixture JSON files.

Run this script when fixtures need to be regenerated. Do not edit the JSON
fixtures directly; update the cases below instead.
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
WIDGETS = ROOT / "widgets"
EVENTS = ROOT / "events"


def write(path: Path, payload: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")


def node(kind: str, data: dict[str, object]) -> dict[str, object]:
    return {"type": kind, "data": data}


def widgets() -> None:
    for fixture in WIDGETS.glob("*.json"):
        fixture.unlink()

    label = node("label", {"label": "Ready", "wrap": True})
    badge = node("badge", {"label": "OK", "kind": "success"})
    status = node("status_dot", {"status": "warning"})

    cases: dict[str, dict[str, object]] = {
        "label": label,
        "header": node("header", {"label": "Network"}),
        "hero": node(
            "hero",
            {
                "id": "vpn",
                "icon": "network-vpn-symbolic",
                "icon_size": 32,
                "title": "VPN",
                "subtitle": "Disconnected",
                "toggle": False,
                "toggle_sensitive": True,
                "separator": True,
                "trailing": badge,
            },
        ),
        "badge": badge,
        "status-dot": status,
        "panel-indicator": node(
            "panel_indicator",
            {
                "id": "net",
                "icon": "network-wireless-symbolic",
                "label": "Wi-Fi",
                "active": True,
                "checked": False,
                "needs_attention": False,
                "extra": status,
            },
        ),
        "empty-state": node("empty_state", {"title": "No devices", "subtitle": "Connect a device to continue"}),
        "spinner": node("spinner", {"spinning": True}),
        "meter": node(
            "meter",
            {
                "label": "Memory",
                "value": 0.51,
                "min": 0.0,
                "max": 1.0,
                "step": 0.01,
                "interactive": False,
            },
        ),
        "separator": node("separator", {}),
        "scroll": node("scroll", {"child": label}),
        "row": node("row", {"children": [label, badge]}),
        "column": node("column", {"children": [label, badge]}),
        "container": node("container", {"children": [label]}),
        "circle-box": node("circle_box", {"color": "#336699"}),
        "boxed-list": node("boxed_list", {"children": [label, badge]}),
        "popover-shell": node(
            "popover_shell",
            {
                "size": "medium",
                "children": [label],
                "footer": [badge],
                "footer_visible": True,
            },
        ),
        "tile": node(
            "tile",
            {
                "id": "wifi",
                "primary": "Wi-Fi",
                "secondary": "Connected",
                "left_icon": "network-wireless-symbolic",
                "right": badge,
                "activatable": True,
            },
        ),
        "segmented-tile": node(
            "segmented_tile",
            {
                "id": "drive",
                "primary": "Backup",
                "secondary": "Mounted",
                "left_icon": "drive-harddisk-symbolic",
                "right": badge,
                "child": node("key_value_grid", {"rows": [{"key": "Size", "value": "1 TB"}]}),
                "expanded": True,
                "activatable": True,
            },
        ),
        "button-row": node("button_row", {"children": [node("tile", {"primary": "Refresh", "activatable": True})]}),
        "switch-tile": node(
            "switch_tile",
            {
                "id": "bluetooth",
                "primary": "Bluetooth",
                "secondary": "On",
                "left_icon": "bluetooth-active-symbolic",
                "active": True,
            },
        ),
        "expander-tile": node(
            "expander_tile",
            {
                "id": "details",
                "primary": "Details",
                "secondary": "2 items",
                "left_icon": "view-list-symbolic",
                "child": node("column", {"children": [label]}),
                "expanded": True,
            },
        ),
        "slider-tile": node(
            "slider_tile",
            {
                "id": "brightness",
                "label": "Brightness",
                "left_icon": "display-brightness-symbolic",
                "value": 0.6,
                "min": 0.0,
                "max": 1.0,
                "step": 0.05,
                "page": 0.1,
                "digits": 0,
                "snap_step": 0.05,
            },
        ),
        "choice-tile": node(
            "choice_tile",
            {
                "id": "balanced",
                "primary": "Balanced",
                "secondary": "Recommended",
                "left_icon": "power-profile-balanced-symbolic",
                "selected": True,
            },
        ),
        "choice-list": node(
            "choice_list",
            {
                "id": "profile",
                "active": "balanced",
                "choices": [
                    {
                        "id": "balanced",
                        "primary": "Balanced",
                        "secondary": "Recommended",
                        "icon": "power-profile-balanced-symbolic",
                    },
                    {
                        "id": "performance",
                        "primary": "Performance",
                        "secondary": "Fast",
                        "icon": "power-profile-performance-symbolic",
                    },
                ],
            },
        ),
        "key-value-grid": node("key_value_grid", {"rows": [{"key": "IPv4", "value": "10.0.0.42"}]}),
        "pager-item": node(
            "pager_item",
            {
                "id": 1,
                "label": "1",
                "appearance": "numbers",
                "active": True,
                "inactive": False,
                "occupied": True,
                "urgent": False,
            },
        ),
        "pager-strip": node(
            "pager_strip",
            {
                "id": "workspaces",
                "placeholder": False,
                "items": [
                    {
                        "id": 1,
                        "label": "1",
                        "appearance": "numbers",
                        "active": True,
                        "inactive": False,
                        "occupied": True,
                        "urgent": False,
                    },
                    {
                        "id": 2,
                        "label": "2",
                        "appearance": "numbers",
                        "active": False,
                        "inactive": True,
                        "occupied": False,
                        "urgent": False,
                    },
                ],
            },
        ),
        "camera-indicator": node("camera_indicator", {"active": True}),
        "mic-indicator": node("mic_indicator", {"active": True}),
        "muted-indicator": node("muted_indicator", {"active": True}),
        "screencast-indicator": node("screencast_indicator", {"active": True, "timer_text": "01:23"}),
        "location-indicator": node("location_indicator", {"active": True}),
        "calendar": node(
            "calendar",
            {
                "id": "calendar",
                "selected_date": "2026-05-22",
                "event_days": ["2026-05-22", "2026-05-24"],
            },
        ),
        "battery-hero": node(
            "battery_hero",
            {
                "icon": "battery-good-symbolic",
                "percentage": "82%",
                "fraction": 0.82,
                "state": "Discharging",
            },
        ),
        "date-hero": node("date_hero", {"weekday": "Friday", "date": "May 22"}),
        "events": node(
            "events",
            {
                "date": "2026-05-22",
                "loading": False,
                "events": [{"id": "standup", "title": "Standup", "start": "09:30", "end": "09:45", "all_day": False}],
            },
        ),
        "weather-forecast-list": node(
            "weather_forecast_list",
            {
                "items": [
                    {
                        "day_name": "Today",
                        "icon": "weather-clear-symbolic",
                        "condition": "Clear",
                        "temperatures": "12 / 20",
                        "is_today": True,
                    }
                ]
            },
        ),
        "weather-hourly-strip": node(
            "weather_hourly_strip",
            {"items": [{"time": "12:00", "icon": "weather-clear-symbolic", "temperature": "18"}]},
        ),
        "world-clock": node(
            "world_clock",
            {
                "rows": [
                    {
                        "name": "UTC",
                        "timezone": "UTC",
                        "time": "12:00",
                        "offset": "+00:00",
                        "day_label": "Today",
                    }
                ]
            },
        ),
        "tree-shared-popover": node(
            "popover_shell",
            {
                "size": "large",
                "children": [
                    node("hero", {"title": "System", "subtitle": "Shared widgets"}),
                    node("boxed_list", {"children": [node("switch_tile", {"id": "wifi", "primary": "Wi-Fi", "active": True})]}),
                ],
            },
        ),
    }

    for name, payload in cases.items():
        write(WIDGETS / f"{name}.json", payload)


def events() -> None:
    fixtures = {
        "click-left": (
            {"id": "refresh", "type": "click", "source": "popover", "button": "left"},
            {"event": "click", "id": "refresh", "button": "left"},
        ),
        "click-no-button": (
            {"id": "refresh", "type": "click", "source": "popover"},
            {"event": "click", "id": "refresh", "button": None},
        ),
        "scroll-down": (
            {"id": "list", "type": "scroll", "source": "popover", "delta_y": 1.5},
            {"event": "scroll", "id": "list", "delta_y": 1.5},
        ),
        "input": (
            {"id": "search", "type": "change", "source": "popover", "value": "hello"},
            {"event": "change", "id": "search", "value": "hello"},
        ),
        "toggle-active-true": (
            {"id": "vpn", "type": "toggle", "source": "popover", "active": True},
            {"event": "toggle", "id": "vpn", "value": True},
        ),
        "toggle-active-false": (
            {"id": "vpn", "type": "toggle", "source": "popover", "active": False},
            {"event": "toggle", "id": "vpn", "value": False},
        ),
        "toggle-via-value-true": (
            {"id": "vpn", "type": "toggle", "source": "popover", "value": True},
            {"event": "toggle", "id": "vpn", "value": True},
        ),
        "toggle-numeric-value-is-false": (
            {"id": "vpn", "type": "toggle", "source": "popover", "value": 1},
            {"event": "toggle", "id": "vpn", "value": False},
        ),
        "change-scale": (
            {"id": "brightness", "type": "change", "source": "popover", "value": 0.6},
            {"event": "change", "id": "brightness", "value": 0.6},
        ),
        "change-dropdown": (
            {"id": "profile", "type": "change", "source": "popover", "value": "balanced"},
            {"event": "change", "id": "profile", "value": "balanced"},
        ),
        "popover-open": (
            {"id": "popover", "type": "open", "source": "popover"},
            {"event": "open", "id": "popover", "open": True},
        ),
        "popover-close": (
            {"id": "popover", "type": "close", "source": "popover"},
            {"event": "close", "id": "popover", "open": False},
        ),
    }

    for name, (incoming, parsed) in fixtures.items():
        write(EVENTS / f"{name}.json", {"incoming": incoming, "parsed": parsed})


def main() -> None:
    widgets()
    events()

    fixtures = sorted(p.name for p in WIDGETS.glob("*.json")) + sorted(
        f"events/{p.name}" for p in EVENTS.glob("*.json")
    )
    for name in fixtures:
        print(name)
    print(f"\nTotal: {len(fixtures)} fixtures")


if __name__ == "__main__":
    main()
