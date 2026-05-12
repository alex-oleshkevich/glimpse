#!/usr/bin/env python3
"""Generate the shared golden-fixture JSON files.

Run this script when fixtures need to be regenerated. Do not edit the JSON
files by hand — edit this script instead. The script encodes the canonical
shape rules documented in README.md.
"""

from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).parent
WIDGETS = ROOT / "widgets"
EVENTS = ROOT / "events"


def write(folder: Path, name: str, value: object) -> None:
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / f"{name}.json"
    encoded = json.dumps(value, indent=2, sort_keys=True) + "\n"
    path.write_text(encoded)


# ---------- widgets ----------


def widgets() -> None:
    write(WIDGETS, "label-basic", {
        "type": "label",
        "data": {"text": "Hello"},
    })
    write(WIDGETS, "label-modifiers", {
        "type": "label",
        "data": {"text": "Hello", "wrap": True, "xalign": 0.5, "selectable": True},
    })
    write(WIDGETS, "button-basic", {
        "type": "button",
        "data": {"id": "go", "label": "Go"},
    })
    write(WIDGETS, "button-with-icon", {
        "type": "button",
        "data": {"id": "go", "label": "Go", "icon": {"name": "go-symbolic"}},
    })
    write(WIDGETS, "button-icon-only", {
        "type": "button",
        "data": {"id": "go", "icon": {"name": "go-symbolic"}},
    })
    write(WIDGETS, "switch-on", {
        "type": "switch",
        "data": {"id": "vpn", "label": "VPN", "active": True},
    })
    write(WIDGETS, "switch-off", {
        "type": "switch",
        "data": {"id": "vpn", "active": False},
    })
    write(WIDGETS, "checkbox-on", {
        "type": "checkbox",
        "data": {"id": "autostart", "label": "Run at login", "active": True},
    })
    write(WIDGETS, "scale", {
        "type": "scale",
        "data": {"id": "brightness", "min": 0.0, "max": 1.0, "step": 0.05, "value": 0.6},
    })
    write(WIDGETS, "dropdown", {
        "type": "dropdown",
        "data": {
            "id": "env",
            "items": [
                {"id": "prod", "label": "Production"},
                {"id": "stage", "label": "Staging"},
            ],
            "selected": 0,
        },
    })
    write(WIDGETS, "dropdown-empty", {
        "type": "dropdown",
        "data": {"id": "env", "items": []},
    })
    write(WIDGETS, "badge", {"type": "badge", "data": {"label": "42%"}})
    write(WIDGETS, "badge-success-variant", {
        "type": "badge",
        "data": {"label": "OK", "variant": "success"},
    })
    write(WIDGETS, "hero-basic", {
        "type": "hero",
        "data": {"title": "Counter", "subtitle": "Value: 0"},
    })
    write(WIDGETS, "hero-with-icon", {
        "type": "hero",
        "data": {
            "title": "VPN",
            "subtitle": "Connected",
            "icon": {"name": "network-vpn-symbolic"},
        },
    })
    write(WIDGETS, "progress", {
        "type": "progress",
        "data": {"value": 0.7, "max": 1.0},
    })
    write(WIDGETS, "progress-with-text", {
        "type": "progress",
        "data": {"value": 0.7, "max": 1.0, "show_text": True, "text": "70%"},
    })
    write(WIDGETS, "spinner-default", {
        "type": "spinner",
        "data": {"spinning": True},
    })
    write(WIDGETS, "spinner-stopped", {
        "type": "spinner",
        "data": {"spinning": False},
    })
    write(WIDGETS, "status-dot", {"type": "status", "data": {}})
    write(WIDGETS, "status-dot-warning", {
        "type": "status",
        "data": {"variant": "warning"},
    })
    write(WIDGETS, "icon", {
        "type": "icon",
        "data": {"icon": {"name": "network-wireless-symbolic"}, "pixel_size": 24},
    })
    write(WIDGETS, "image-by-name", {
        "type": "image",
        "data": {"icon": {"name": "user-info-symbolic"}},
    })
    write(WIDGETS, "image-by-path", {
        "type": "image",
        "data": {"icon": {"path": "/home/me/avatar.png"}, "pixel_size": 64},
    })
    write(WIDGETS, "separator", {"type": "separator", "data": {}})
    write(WIDGETS, "box-vertical", {
        "type": "box",
        "data": {"orientation": "vertical", "spacing": 8, "children": []},
    })
    write(WIDGETS, "box-horizontal", {
        "type": "box",
        "data": {"orientation": "horizontal", "spacing": 4, "children": []},
    })
    write(WIDGETS, "row", {
        "type": "row",
        "data": {"spacing": 8, "children": []},
    })
    write(WIDGETS, "column", {
        "type": "column",
        "data": {"spacing": 8, "children": []},
    })
    write(WIDGETS, "grid", {
        "type": "grid",
        "data": {
            "row_spacing": 4,
            "column_spacing": 4,
            "children": [
                {
                    "row": 0,
                    "column": 0,
                    "width": 1,
                    "height": 1,
                    "child": {"type": "label", "data": {"text": "A"}},
                },
                {
                    "row": 0,
                    "column": 1,
                    "width": 2,
                    "height": 1,
                    "child": {"type": "label", "data": {"text": "B"}},
                },
            ],
        },
    })
    write(WIDGETS, "scroll", {
        "type": "scroll",
        "data": {"child": {"type": "label", "data": {"text": "scrollable"}}},
    })
    write(WIDGETS, "card", {
        "type": "card",
        "data": {
            "children": [
                {"type": "label", "data": {"text": "in card"}},
            ],
        },
    })
    write(WIDGETS, "card-empty", {
        "type": "card",
        "data": {"children": []},
    })
    write(WIDGETS, "section-basic", {
        "type": "section",
        "data": {
            "header": {"title": "System"},
            "body": [
                {"type": "label", "data": {"text": "uptime"}},
            ],
        },
    })
    write(WIDGETS, "section-empty-body", {
        "type": "section",
        "data": {"header": {"title": "Empty"}, "body": []},
    })
    write(WIDGETS, "collapsible-closed", {
        "type": "collapsible",
        "data": {
            "header": {"title": "Advanced"},
            "expanded": False,
            "body": [],
        },
    })
    write(WIDGETS, "collapsible-open-with-body", {
        "type": "collapsible",
        "data": {
            "header": {"title": "Advanced"},
            "expanded": True,
            "body": [
                {"type": "label", "data": {"text": "inside"}},
            ],
        },
    })
    write(WIDGETS, "item-basic", {
        "type": "item",
        "data": {"label": "Plain", "clickable": False, "menu": []},
    })
    write(WIDGETS, "item-clickable", {
        "type": "item",
        "data": {"id": "run", "label": "Run", "clickable": True, "menu": []},
    })
    write(WIDGETS, "item-with-menu", {
        "type": "item",
        "data": {
            "id": "wifi-home",
            "label": "home-5G",
            "clickable": True,
            "menu": [
                {"id": "forget", "label": "Forget"},
                {"id": "details", "label": "Details", "enabled": False},
            ],
        },
    })
    write(WIDGETS, "collapsible-item", {
        "type": "collapsible_item",
        "data": {
            "label": "Devices",
            "expanded": False,
            "body": [],
        },
    })
    write(WIDGETS, "action-row", {
        "type": "action_row",
        "data": {"id": "go", "title": "Connect", "subtitle": "", "meta": ""},
    })
    write(WIDGETS, "action-row-with-meta", {
        "type": "action_row",
        "data": {
            "id": "go",
            "title": "Connect",
            "subtitle": "wg0",
            "meta": "4 routes",
            "icon": {"name": "network-vpn-symbolic"},
        },
    })
    write(WIDGETS, "action-menu", {
        "type": "action_menu",
        "data": {
            "header": "Power profile",
            "items": [
                {"id": "saver", "label": "Power Saver", "checked": False},
                {"id": "balanced", "label": "Balanced", "checked": True},
            ],
        },
    })
    write(WIDGETS, "action-menu-empty", {
        "type": "action_menu",
        "data": {"items": []},
    })
    write(WIDGETS, "detail-grid", {
        "type": "detail_grid",
        "data": {
            "rows": [
                {"key": "SSID", "value": "home-5G"},
                {"key": "IPv4", "value": "10.0.0.42"},
            ],
        },
    })
    write(WIDGETS, "detail-grid-empty", {
        "type": "detail_grid",
        "data": {"rows": []},
    })
    write(WIDGETS, "empty-state", {
        "type": "empty_state",
        "data": {"title": "Nothing here", "subtitle": ""},
    })
    write(WIDGETS, "empty-state-with-subtitle", {
        "type": "empty_state",
        "data": {"title": "Nothing here", "subtitle": "Plug in a device."},
    })
    write(WIDGETS, "meter", {
        "type": "meter",
        "data": {
            "label": "Memory",
            "value": 0.51,
            "min": 0.0,
            "max": 1.0,
            "step": 0.01,
            "interactive": False,
        },
    })
    write(WIDGETS, "meter-interactive", {
        "type": "meter",
        "data": {
            "icon": {"name": "audio-volume-medium-symbolic"},
            "label": "Volume",
            "value": 0.42,
            "min": 0.0,
            "max": 1.0,
            "step": 0.01,
            "text": "42%",
            "interactive": True,
        },
    })
    write(WIDGETS, "copyable", {
        "type": "copyable",
        "data": {"label": "IPv4", "value": "10.0.0.42"},
    })
    write(WIDGETS, "toast", {
        "type": "toast",
        "data": {"title": "Saved", "message": ""},
    })
    write(WIDGETS, "toast-with-action", {
        "type": "toast",
        "data": {
            "icon": {"name": "dialog-warning-symbolic"},
            "title": "Update available",
            "message": "Version 0.8 is available.",
            "action": {"id": "update", "label": "Update"},
        },
    })
    write(WIDGETS, "common-props-all", {
        "type": "label",
        "data": {
            "text": "marked",
            "id": "marked",
            "visible": False,
            "hexpand": True,
            "vexpand": True,
            "halign": "center",
            "valign": "end",
            "tooltip": "details",
            "variant": "warning",
        },
    })


# ---------- combined trees ----------


def trees() -> None:
    write(WIDGETS, "tree-hero-column-section", {
        "type": "column",
        "data": {
            "spacing": 8,
            "children": [
                {
                    "type": "hero",
                    "data": {"title": "Counter", "subtitle": "Value: 0"},
                },
                {
                    "type": "section",
                    "data": {
                        "header": {"title": "Controls"},
                        "body": [
                            {"type": "label", "data": {"text": "Current"}},
                            {
                                "type": "button",
                                "data": {"id": "increment", "label": "Increment"},
                            },
                        ],
                    },
                },
            ],
        },
    })
    write(WIDGETS, "tree-card-with-grid", {
        "type": "card",
        "data": {
            "children": [
                {
                    "type": "grid",
                    "data": {
                        "row_spacing": 4,
                        "column_spacing": 8,
                        "children": [
                            {
                                "row": 0,
                                "column": 0,
                                "width": 1,
                                "height": 1,
                                "child": {"type": "label", "data": {"text": "K"}},
                            },
                            {
                                "row": 0,
                                "column": 1,
                                "width": 1,
                                "height": 1,
                                "child": {"type": "badge", "data": {"label": "V"}},
                            },
                        ],
                    },
                },
            ],
        },
    })


# ---------- events ----------


def events() -> None:
    """For events, each fixture is a pair (incoming_line_payload, parsed_event)."""

    def evt(name: str, incoming: dict, parsed: dict) -> None:
        write(EVENTS, name, {"incoming": incoming, "parsed": parsed})

    evt(
        "click-left",
        {"id": "submit", "type": "click", "source": "popover", "button": "left"},
        {"kind": "click", "id": "submit", "button": "left"},
    )
    evt(
        "click-no-button",
        {"id": "submit", "type": "click", "source": "popover"},
        {"kind": "click", "id": "submit", "button": None},
    )
    evt(
        "scroll-down",
        {"id": "cpu", "type": "scroll", "source": "status", "delta_y": -1.5},
        {"kind": "scroll", "id": "cpu", "delta_y": -1.5},
    )
    evt(
        "input",
        {"id": "filter", "type": "input", "source": "popover", "text": "foo"},
        {"kind": "input", "id": "filter", "text": "foo"},
    )
    evt(
        "toggle-active-true",
        {"id": "vpn", "type": "toggle", "source": "popover", "active": True},
        {"kind": "toggle", "id": "vpn", "value": True},
    )
    evt(
        "toggle-active-false",
        {"id": "vpn", "type": "toggle", "source": "popover", "active": False},
        {"kind": "toggle", "id": "vpn", "value": False},
    )
    evt(
        "toggle-via-value-true",
        {"id": "vpn", "type": "toggle", "source": "popover", "value": True},
        {"kind": "toggle", "id": "vpn", "value": True},
    )
    evt(
        "toggle-numeric-value-is-false",
        {"id": "vpn", "type": "toggle", "source": "popover", "value": 0.5},
        {"kind": "toggle", "id": "vpn", "value": False},
    )
    evt(
        "change-scale",
        {"id": "brightness", "type": "change", "source": "popover", "value": 0.72},
        {"kind": "change", "id": "brightness", "value": 0.72},
    )
    evt(
        "change-dropdown",
        {
            "id": "env",
            "type": "change",
            "source": "popover",
            "value": {"id": "stage", "label": "Staging", "index": 1},
        },
        {
            "kind": "change",
            "id": "env",
            "value": {"id": "stage", "label": "Staging", "index": 1},
        },
    )
    evt(
        "popover-open",
        {"id": "popover", "type": "open", "source": "popover"},
        {"kind": "popover", "open": True},
    )
    evt(
        "popover-close",
        {"id": "popover", "type": "close", "source": "popover"},
        {"kind": "popover", "open": False},
    )


if __name__ == "__main__":
    widgets()
    trees()
    events()
    fixtures = sorted(p.name for p in WIDGETS.glob("*.json")) + sorted(
        p.name for p in EVENTS.glob("*.json")
    )
    for name in fixtures:
        print(name)
    print(f"\nTotal: {len(fixtures)} fixtures")
