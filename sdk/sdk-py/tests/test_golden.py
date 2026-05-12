"""Golden cross-SDK fixture tests.

Each case builds a widget and asserts its JSON serialization equals the
corresponding fixture under ../../fixtures/widgets/.
Event tests parse the canonical incoming payload and assert the documented
typed event is returned.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from glimpse_sdk import (
    ActionMenu,
    ActionMenuItem,
    ActionRow,
    Align,
    Badge,
    Box,
    Button,
    Card,
    Checkbox,
    Collapsible,
    CollapsibleItem,
    Column,
    Copyable,
    DetailGrid,
    DetailGridItem,
    Dropdown,
    DropdownItem,
    EmptyState,
    Grid,
    GridChild,
    Hero,
    Icon,
    IconWidget,
    Image,
    Item,
    Label,
    MenuItem,
    Meter,
    Orientation,
    Progress,
    Row,
    Scale,
    Scroll,
    Section,
    Separator,
    Spinner,
    StatusDot,
    Switch,
    Toast,
    ToastAction,
    Variant,
)
from glimpse_sdk.events import (
    ChangeEvent,
    ClickEvent,
    InputEvent,
    PopoverEvent,
    ScrollEvent,
    ToggleEvent,
    parse_callback_event,
)
from glimpse_sdk.widgets import Header

FIXTURES = Path(__file__).resolve().parents[2] / "fixtures"


def load(rel: str) -> object:
    return json.loads((FIXTURES / rel).read_text())


class GoldenWidgetTests(unittest.TestCase):
    maxDiff = None

    def _assert_widget(self, name: str, widget: object) -> None:
        expected = load(f"widgets/{name}.json")
        got = widget.to_protocol()
        self.assertEqual(got, expected, f"fixture mismatch for widgets/{name}.json")

    def test_label_basic(self) -> None:
        self._assert_widget("label-basic", Label(text="Hello"))

    def test_label_modifiers(self) -> None:
        self._assert_widget(
            "label-modifiers",
            Label(text="Hello", wrap=True, xalign=0.5, selectable=True),
        )

    def test_button_basic(self) -> None:
        self._assert_widget("button-basic", Button(id="go", label="Go"))

    def test_button_with_icon(self) -> None:
        self._assert_widget(
            "button-with-icon",
            Button(id="go", label="Go", icon=Icon.name("go-symbolic")),
        )

    def test_button_icon_only(self) -> None:
        self._assert_widget(
            "button-icon-only",
            Button(id="go", icon=Icon.name("go-symbolic")),
        )

    def test_switch_on(self) -> None:
        self._assert_widget("switch-on", Switch(id="vpn", label="VPN", active=True))

    def test_switch_off(self) -> None:
        self._assert_widget("switch-off", Switch(id="vpn"))

    def test_checkbox_on(self) -> None:
        self._assert_widget(
            "checkbox-on", Checkbox(id="autostart", label="Run at login", active=True)
        )

    def test_scale(self) -> None:
        self._assert_widget(
            "scale", Scale(id="brightness", min=0.0, max=1.0, step=0.05, value=0.6)
        )

    def test_dropdown(self) -> None:
        self._assert_widget(
            "dropdown",
            Dropdown(
                id="env",
                items=[
                    DropdownItem(id="prod", label="Production"),
                    DropdownItem(id="stage", label="Staging"),
                ],
                selected=0,
            ),
        )

    def test_dropdown_empty(self) -> None:
        self._assert_widget("dropdown-empty", Dropdown(id="env"))

    def test_badge(self) -> None:
        self._assert_widget("badge", Badge(label="42%"))

    def test_badge_success_variant(self) -> None:
        self._assert_widget(
            "badge-success-variant", Badge(label="OK", variant=Variant.SUCCESS)
        )

    def test_hero_basic(self) -> None:
        self._assert_widget("hero-basic", Hero(title="Counter", subtitle="Value: 0"))

    def test_hero_with_icon(self) -> None:
        self._assert_widget(
            "hero-with-icon",
            Hero(title="VPN", subtitle="Connected", icon=Icon.name("network-vpn-symbolic")),
        )

    def test_progress(self) -> None:
        self._assert_widget("progress", Progress(value=0.7, max=1.0))

    def test_progress_with_text(self) -> None:
        self._assert_widget(
            "progress-with-text",
            Progress(value=0.7, max=1.0, show_text=True, text="70%"),
        )

    def test_spinner_default(self) -> None:
        self._assert_widget("spinner-default", Spinner())

    def test_spinner_stopped(self) -> None:
        self._assert_widget("spinner-stopped", Spinner(spinning=False))

    def test_status_dot(self) -> None:
        self._assert_widget("status-dot", StatusDot())

    def test_status_dot_warning(self) -> None:
        self._assert_widget("status-dot-warning", StatusDot(variant=Variant.WARNING))

    def test_icon(self) -> None:
        self._assert_widget(
            "icon", IconWidget(icon=Icon.name("network-wireless-symbolic"), pixel_size=24)
        )

    def test_image_by_name(self) -> None:
        self._assert_widget("image-by-name", Image(icon=Icon.name("user-info-symbolic")))

    def test_image_by_path(self) -> None:
        self._assert_widget(
            "image-by-path", Image(icon=Icon.path("/home/me/avatar.png"), pixel_size=64)
        )

    def test_separator(self) -> None:
        self._assert_widget("separator", Separator())

    def test_box_vertical(self) -> None:
        self._assert_widget("box-vertical", Box.vertical([], spacing=8))

    def test_box_horizontal(self) -> None:
        self._assert_widget("box-horizontal", Box.horizontal([], spacing=4))

    def test_row(self) -> None:
        self._assert_widget("row", Row(spacing=8))

    def test_column(self) -> None:
        self._assert_widget("column", Column(spacing=8))

    def test_grid(self) -> None:
        self._assert_widget(
            "grid",
            Grid(
                row_spacing=4,
                column_spacing=4,
                children=[
                    GridChild(row=0, column=0, child=Label(text="A")),
                    GridChild(row=0, column=1, width=2, child=Label(text="B")),
                ],
            ),
        )

    def test_scroll(self) -> None:
        self._assert_widget("scroll", Scroll(child=Label(text="scrollable")))

    def test_card(self) -> None:
        self._assert_widget("card", Card(children=[Label(text="in card")]))

    def test_card_empty(self) -> None:
        self._assert_widget("card-empty", Card())

    def test_section_basic(self) -> None:
        self._assert_widget(
            "section-basic",
            Section(header=Header(title="System"), body=[Label(text="uptime")]),
        )

    def test_section_empty_body(self) -> None:
        self._assert_widget("section-empty-body", Section(header=Header(title="Empty")))

    def test_collapsible_closed(self) -> None:
        self._assert_widget(
            "collapsible-closed",
            Collapsible(header=Header(title="Advanced"), expanded=False),
        )

    def test_collapsible_open_with_body(self) -> None:
        self._assert_widget(
            "collapsible-open-with-body",
            Collapsible(
                header=Header(title="Advanced"),
                expanded=True,
                body=[Label(text="inside")],
            ),
        )

    def test_item_basic(self) -> None:
        self._assert_widget("item-basic", Item(label="Plain"))

    def test_item_clickable(self) -> None:
        self._assert_widget("item-clickable", Item(id="run", label="Run", clickable=True))

    def test_item_with_menu(self) -> None:
        self._assert_widget(
            "item-with-menu",
            Item(
                id="wifi-home",
                label="home-5G",
                clickable=True,
                menu=[
                    MenuItem(id="forget", label="Forget"),
                    MenuItem(id="details", label="Details", enabled=False),
                ],
            ),
        )

    def test_collapsible_item(self) -> None:
        self._assert_widget(
            "collapsible-item",
            CollapsibleItem(label="Devices", expanded=False),
        )

    def test_action_row(self) -> None:
        self._assert_widget("action-row", ActionRow(id="go", title="Connect"))

    def test_action_row_with_meta(self) -> None:
        self._assert_widget(
            "action-row-with-meta",
            ActionRow(
                id="go",
                title="Connect",
                subtitle="wg0",
                meta="4 routes",
                icon=Icon.name("network-vpn-symbolic"),
            ),
        )

    def test_action_menu(self) -> None:
        self._assert_widget(
            "action-menu",
            ActionMenu(
                header="Power profile",
                items=[
                    ActionMenuItem(id="saver", label="Power Saver", checked=False),
                    ActionMenuItem(id="balanced", label="Balanced", checked=True),
                ],
            ),
        )

    def test_action_menu_empty(self) -> None:
        self._assert_widget("action-menu-empty", ActionMenu())

    def test_detail_grid(self) -> None:
        self._assert_widget(
            "detail-grid",
            DetailGrid(
                rows=[
                    DetailGridItem(key="SSID", value="home-5G"),
                    DetailGridItem(key="IPv4", value="10.0.0.42"),
                ]
            ),
        )

    def test_detail_grid_empty(self) -> None:
        self._assert_widget("detail-grid-empty", DetailGrid())

    def test_empty_state(self) -> None:
        self._assert_widget("empty-state", EmptyState(title="Nothing here"))

    def test_empty_state_with_subtitle(self) -> None:
        self._assert_widget(
            "empty-state-with-subtitle",
            EmptyState(title="Nothing here", subtitle="Plug in a device."),
        )

    def test_meter(self) -> None:
        self._assert_widget("meter", Meter(label="Memory", value=0.51, max=1.0))

    def test_meter_interactive(self) -> None:
        self._assert_widget(
            "meter-interactive",
            Meter(
                icon=Icon.name("audio-volume-medium-symbolic"),
                label="Volume",
                value=0.42,
                max=1.0,
                text="42%",
                interactive=True,
            ),
        )

    def test_copyable(self) -> None:
        self._assert_widget("copyable", Copyable(label="IPv4", value="10.0.0.42"))

    def test_toast(self) -> None:
        self._assert_widget("toast", Toast(title="Saved"))

    def test_toast_with_action(self) -> None:
        self._assert_widget(
            "toast-with-action",
            Toast(
                icon=Icon.name("dialog-warning-symbolic"),
                title="Update available",
                message="Version 0.8 is available.",
                action=ToastAction(id="update", label="Update"),
            ),
        )

    def test_common_props_all(self) -> None:
        self._assert_widget(
            "common-props-all",
            Label(
                text="marked",
                id="marked",
                visible=False,
                hexpand=True,
                vexpand=True,
                halign=Align.CENTER,
                valign=Align.END,
                tooltip="details",
                variant=Variant.WARNING,
            ),
        )

    def test_tree_hero_column_section(self) -> None:
        self._assert_widget(
            "tree-hero-column-section",
            Column(
                spacing=8,
                children=[
                    Hero(title="Counter", subtitle="Value: 0"),
                    Section(
                        header=Header(title="Controls"),
                        body=[
                            Label(text="Current"),
                            Button(id="increment", label="Increment"),
                        ],
                    ),
                ],
            ),
        )

    def test_tree_card_with_grid(self) -> None:
        self._assert_widget(
            "tree-card-with-grid",
            Card(
                children=[
                    Grid(
                        row_spacing=4,
                        column_spacing=8,
                        children=[
                            GridChild(row=0, column=0, child=Label(text="K")),
                            GridChild(row=0, column=1, child=Badge(label="V")),
                        ],
                    )
                ]
            ),
        )


class GoldenEventTests(unittest.TestCase):
    def _fixture(self, name: str) -> tuple[dict, dict]:
        data = load(f"events/{name}.json")
        return data["incoming"], data["parsed"]

    def test_click_left(self) -> None:
        incoming, parsed = self._fixture("click-left")
        event = parse_callback_event(incoming)
        self.assertIsInstance(event, ClickEvent)
        self.assertEqual(event.id, parsed["id"])
        self.assertEqual(event.button, parsed["button"])

    def test_click_no_button(self) -> None:
        incoming, _ = self._fixture("click-no-button")
        event = parse_callback_event(incoming)
        self.assertIsInstance(event, ClickEvent)
        self.assertIsNone(event.button)

    def test_scroll_down(self) -> None:
        incoming, parsed = self._fixture("scroll-down")
        event = parse_callback_event(incoming)
        self.assertIsInstance(event, ScrollEvent)
        self.assertEqual(event.delta_y, parsed["delta_y"])

    def test_input(self) -> None:
        incoming, parsed = self._fixture("input")
        event = parse_callback_event(incoming)
        self.assertIsInstance(event, InputEvent)
        self.assertEqual(event.text, parsed["text"])

    def test_toggle_active_true(self) -> None:
        incoming, _ = self._fixture("toggle-active-true")
        event = parse_callback_event(incoming)
        self.assertIsInstance(event, ToggleEvent)
        self.assertTrue(event.value)

    def test_toggle_active_false(self) -> None:
        incoming, _ = self._fixture("toggle-active-false")
        event = parse_callback_event(incoming)
        self.assertIsInstance(event, ToggleEvent)
        self.assertFalse(event.value)

    def test_toggle_via_value_true(self) -> None:
        incoming, _ = self._fixture("toggle-via-value-true")
        event = parse_callback_event(incoming)
        self.assertIsInstance(event, ToggleEvent)
        self.assertTrue(event.value)

    def test_toggle_numeric_value_is_false(self) -> None:
        incoming, _ = self._fixture("toggle-numeric-value-is-false")
        event = parse_callback_event(incoming)
        self.assertIsInstance(event, ToggleEvent)
        self.assertFalse(event.value)

    def test_change_scale(self) -> None:
        incoming, parsed = self._fixture("change-scale")
        event = parse_callback_event(incoming)
        self.assertIsInstance(event, ChangeEvent)
        self.assertEqual(event.value, parsed["value"])

    def test_change_dropdown(self) -> None:
        incoming, parsed = self._fixture("change-dropdown")
        event = parse_callback_event(incoming)
        self.assertIsInstance(event, ChangeEvent)
        self.assertEqual(event.value, parsed["value"])

    def test_popover_open(self) -> None:
        incoming, _ = self._fixture("popover-open")
        event = parse_callback_event(incoming)
        self.assertIsInstance(event, PopoverEvent)
        self.assertTrue(event.open)

    def test_popover_close(self) -> None:
        incoming, _ = self._fixture("popover-close")
        event = parse_callback_event(incoming)
        self.assertIsInstance(event, PopoverEvent)
        self.assertFalse(event.open)


if __name__ == "__main__":
    unittest.main()
