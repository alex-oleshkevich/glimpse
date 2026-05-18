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
    write(WIDGETS, "text-styled", {
        "type": "text",
        "data": {
            "text": "Aligned text",
            "color": "accent",
            "size": "lg",
            "weight": "bold",
            "align": "center",
        },
    })
    write(WIDGETS, "button-basic", {
        "type": "button",
        "data": {"id": "go", "label": "Go"},
    })
    write(WIDGETS, "button-with-icon", {
        "type": "button",
        "data": {"id": "go", "label": "Go", "icon": "go-symbolic"},
    })
    write(WIDGETS, "button-icon-only", {
        "type": "button",
        "data": {"id": "go", "icon": "go-symbolic"},
    })
    write(WIDGETS, "button-primary", {
        "type": "button",
        "data": {"id": "go", "label": "Go", "variant": "primary"},
    })
    write(WIDGETS, "button-disabled", {
        "type": "button",
        "data": {"id": "go", "label": "Go", "enabled": False},
    })
    write(WIDGETS, "link-button", {
        "type": "link_button",
        "data": {"uri": "https://example.com"},
    })
    write(WIDGETS, "link-button-label", {
        "type": "link_button",
        "data": {"uri": "https://example.com/docs", "label": "Docs"},
    })
    write(WIDGETS, "expander", {
        "type": "expander",
        "data": {
            "label": "Details",
            "expanded": False,
            "child": {"type": "text", "data": {"text": "More"}},
        },
    })
    write(WIDGETS, "expander-expanded", {
        "type": "expander",
        "data": {
            "label": "Details",
            "expanded": True,
            "child": {"type": "text", "data": {"text": "More"}},
        },
    })
    write(WIDGETS, "level-bar", {
        "type": "level_bar",
        "data": {"value": 0.7, "min": 0.0, "max": 1.0, "mode": "continuous"},
    })
    write(WIDGETS, "switch-on", {
        "type": "switch",
        "data": {"id": "vpn", "label": "VPN", "active": True},
    })
    write(WIDGETS, "switch-off", {
        "type": "switch",
        "data": {"id": "vpn", "active": False},
    })
    write(WIDGETS, "toggle-button-on", {
        "type": "toggle_button",
        "data": {"id": "wifi", "label": "Wi-Fi", "active": True},
    })
    write(WIDGETS, "toggle-button-off", {
        "type": "toggle_button",
        "data": {"id": "wifi", "active": False},
    })
    write(WIDGETS, "toggle-button-with-icon", {
        "type": "toggle_button",
        "data": {"id": "wifi", "icon": "network-wireless-symbolic", "active": False},
    })
    write(WIDGETS, "checkbox-on", {
        "type": "checkbox",
        "data": {"id": "autostart", "label": "Run at login", "active": True},
    })
    write(WIDGETS, "slider", {
        "type": "slider",
        "data": {"id": "brightness", "min": 0.0, "max": 1.0, "step": 0.05, "value": 0.6},
    })
    write(WIDGETS, "select", {
        "type": "select",
        "data": {
            "id": "env",
            "items": [
                {"id": "prod", "label": "Production"},
                {"id": "stage", "label": "Staging"},
            ],
            "selected": 0,
        },
    })
    write(WIDGETS, "select-empty", {
        "type": "select",
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
            "icon": "network-vpn-symbolic",
        },
    })
    write(WIDGETS, "hero-with-switch", {
        "type": "hero",
        "data": {
            "title": "VPN",
            "subtitle": "Connected",
            "id": "vpn-toggle",
            "switch": True,
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
    write(WIDGETS, "pager-item-number-active", {
        "type": "pager_item",
        "data": {
            "id": "workspace-1",
            "appearance": "numbers",
            "label": "1",
            "active": True,
            "inactive": False,
            "occupied": False,
            "urgent": False,
        },
    })
    write(WIDGETS, "pager-strip", {
        "type": "pager_strip",
        "data": {
            "items": [
                {
                    "id": "workspace-1",
                    "appearance": "numbers",
                    "label": "1",
                    "active": True,
                    "inactive": False,
                    "occupied": False,
                    "urgent": False,
                },
                {
                    "id": "workspace-2",
                    "appearance": "numbers",
                    "label": "2",
                    "active": False,
                    "inactive": False,
                    "occupied": True,
                    "urgent": False,
                },
                {
                    "id": "workspace-3",
                    "appearance": "dots",
                    "label": "",
                    "active": False,
                    "inactive": False,
                    "occupied": False,
                    "urgent": True,
                },
            ],
        },
    })
    write(WIDGETS, "icon-by-name", {
        "type": "icon",
        "data": {"icon": "user-info-symbolic"},
    })
    write(WIDGETS, "picture", {
        "type": "picture",
        "data": {"path": "/home/me/photo.png"},
    })
    write(WIDGETS, "picture-content-fit", {
        "type": "picture",
        "data": {"path": "/home/me/photo.png", "content_fit": "cover"},
    })
    write(WIDGETS, "separator", {"type": "separator", "data": {}})
    write(WIDGETS, "row", {
        "type": "row",
        "data": {"spacing": 4, "children": []},
    })
    write(WIDGETS, "column", {
        "type": "column",
        "data": {"spacing": 4, "children": []},
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
                    "child": {"type": "text", "data": {"text": "A"}},
                },
                {
                    "row": 0,
                    "column": 1,
                    "width": 2,
                    "height": 1,
                    "child": {"type": "text", "data": {"text": "B"}},
                },
            ],
        },
    })
    write(WIDGETS, "scroll", {
        "type": "scroll",
        "data": {"child": {"type": "text", "data": {"text": "scrollable"}}},
    })
    write(WIDGETS, "card", {
        "type": "card",
        "data": {
            "child": {"type": "text", "data": {"text": "in card"}},
        },
    })
    write(WIDGETS, "card-empty", {
        "type": "card",
        "data": {},
    })
    write(WIDGETS, "container-styled", {
        "type": "container",
        "data": {
            "child": {"type": "text", "data": {"text": "contained"}},
            "width": 220,
            "height": 80,
            "min_width": 180,
            "min_height": 48,
            "margin": "xs",
            "margin_top": "sm",
            "padding": "md",
            "padding_left": "lg",
            "background": "surface_raised",
            "color": "fg",
            "border_radius": "md",
            "border_width": "thin",
            "border_color": "border",
            "font_size": "sm",
            "font_weight": "semibold",
        },
    })
    write(WIDGETS, "property-list", {
        "type": "property_list",
        "data": {
            "rows": [
                {"key": "IPv4", "value": "10.0.0.42"},
                {"key": "SSID", "value": "home-5G"},
            ],
        },
    })
    write(WIDGETS, "property-list-title", {
        "type": "property_list",
        "data": {
            "title": "Network",
            "rows": [
                {"key": "IPv4", "value": "10.0.0.42"},
                {"key": "SSID", "value": "home-5G"},
            ],
        },
    })
    write(WIDGETS, "property-list-empty", {
        "type": "property_list",
        "data": {"rows": []},
    })
    write(WIDGETS, "item", {
        "type": "item",
        "data": {"label": "Wi-Fi"},
    })
    _icon_left = {"type": "icon", "data": {"icon": "network-wireless-symbolic", "pixel_size": 16}}
    write(WIDGETS, "item-with-right", {
        "type": "item",
        "data": {
            "left": _icon_left,
            "label": "Wi-Fi",
            "sublabel": "Connected",
            "right": {"type": "badge", "data": {"label": "home-5G"}},
        },
    })
    write(WIDGETS, "action-item", {
        "type": "action_item",
        "data": {"id": "wifi", "label": "Wi-Fi"},
    })
    write(WIDGETS, "action-item-with-right", {
        "type": "action_item",
        "data": {
            "id": "wifi",
            "left": _icon_left,
            "label": "Wi-Fi",
            "sublabel": "Connected",
            "right": {"type": "badge", "data": {"label": "home-5G"}},
        },
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
            "id": "volume",
            "icon": "audio-volume-medium-symbolic",
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
    write(WIDGETS, "common-props-all", {
        "type": "text",
        "data": {
            "text": "marked",
            "visible": False,
            "hexpand": True,
            "vexpand": True,
            "halign": "center",
            "valign": "end",
            "tooltip": "details",
            "css_classes": ["marked"],
            "styles": {"font-weight": "600", "margin-top": "2px"},
        },
    })


    write(WIDGETS, "popover-scaffold-basic", {
        "type": "popover_scaffold",
        "data": {
            "body": {"type": "text", "data": {"text": "Content"}},
            "size": "medium",
        },
    })
    write(WIDGETS, "popover-scaffold-with-hero", {
        "type": "popover_scaffold",
        "data": {
            "hero": {"type": "hero", "data": {"subtitle": "Connected", "title": "VPN"}},
            "body": {"type": "text", "data": {"text": "Content"}},
            "size": "large",
        },
    })


# ---------- combined trees ----------


def trees() -> None:
    write(WIDGETS, "tree-hero-column-card", {
        "type": "column",
        "data": {
            "spacing": 4,
            "children": [
                {
                    "type": "hero",
                    "data": {"title": "Counter", "subtitle": "Value: 0"},
                },
                {
                    "type": "card",
                    "data": {
                        "child": {
                            "type": "column",
                            "data": {
                                "children": [
                                    {"type": "text", "data": {"text": "Current"}},
                                    {
                                        "type": "button",
                                        "data": {"id": "increment", "label": "Increment"},
                                    },
                                ],
                                "spacing": 4,
                            },
                        },
                    },
                },
            ],
        },
    })
    write(WIDGETS, "tree-card-with-grid", {
        "type": "card",
        "data": {
            "child": {
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
                            "child": {"type": "text", "data": {"text": "K"}},
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
