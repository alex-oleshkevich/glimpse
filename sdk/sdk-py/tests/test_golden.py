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
    ActionItem,
    Align,
    Badge,
    BorderWidth,
    Button,
    ButtonVariant,
    Card,
    Checkbox,
    Color,
    Column,
    Container,
    Copyable,
    EmptyState,
    Expander,
    FontSize,
    FontWeight,
    Grid,
    GridChild,
    Hero,
    Icon,
    Item,
    LevelBar,
    LinkButton,
    Meter,
    Orientation,
    PagerAppearance,
    PagerItem,
    PagerStrip,
    Picture,
    PopoverScaffold,
    PopoverSize,
    Progress,
    PropertyList,
    Radius,
    Row,
    Scroll,
    Select,
    Separator,
    Slider,
    Space,
    Spinner,
    StatusDot,
    StatusVariant,
    Switch,
    Text,
    TextAlign,
    ToggleButton,
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
FIXTURES = Path(__file__).resolve().parents[2] / "fixtures"


def load(rel: str) -> object:
    return json.loads((FIXTURES / rel).read_text())


class GoldenWidgetTests(unittest.TestCase):
    maxDiff = None

    def _assert_widget(self, name: str, widget: object) -> None:
        expected = load(f"widgets/{name}.json")
        got = widget.to_protocol()
        self.assertEqual(got, expected, f"fixture mismatch for widgets/{name}.json")

    def test_text_styled(self) -> None:
        self._assert_widget(
            "text-styled",
            Text(
                text="Aligned text",
                color=Color.ACCENT,
                size=FontSize.LG,
                weight=FontWeight.BOLD,
                align=TextAlign.CENTER,
            ),
        )

    def test_button_basic(self) -> None:
        self._assert_widget("button-basic", Button(id="go", label="Go"))

    def test_button_with_icon(self) -> None:
        self._assert_widget(
            "button-with-icon",
            Button(id="go", label="Go", icon="go-symbolic"),
        )

    def test_button_icon_only(self) -> None:
        self._assert_widget(
            "button-icon-only",
            Button(id="go", icon="go-symbolic"),
        )

    def test_button_primary(self) -> None:
        self._assert_widget("button-primary", Button(id="go", label="Go", variant=ButtonVariant.PRIMARY))

    def test_button_disabled(self) -> None:
        self._assert_widget("button-disabled", Button(id="go", label="Go", enabled=False))

    def test_link_button(self) -> None:
        self._assert_widget("link-button", LinkButton(uri="https://example.com"))

    def test_link_button_label(self) -> None:
        self._assert_widget(
            "link-button-label",
            LinkButton(uri="https://example.com/docs", label="Docs"),
        )

    def test_expander(self) -> None:
        self._assert_widget("expander", Expander(label="Details", child=Text(text="More")))

    def test_expander_expanded(self) -> None:
        self._assert_widget(
            "expander-expanded",
            Expander(label="Details", expanded=True, child=Text(text="More")),
        )

    def test_level_bar(self) -> None:
        self._assert_widget(
            "level-bar",
            LevelBar(value=0.7, min=0.0, max=1.0, mode="continuous"),
        )

    def test_switch_on(self) -> None:
        self._assert_widget("switch-on", Switch(id="vpn", label="VPN", active=True))

    def test_switch_off(self) -> None:
        self._assert_widget("switch-off", Switch(id="vpn"))

    def test_toggle_button_on(self) -> None:
        self._assert_widget("toggle-button-on", ToggleButton(id="wifi", label="Wi-Fi", active=True))

    def test_toggle_button_off(self) -> None:
        self._assert_widget("toggle-button-off", ToggleButton(id="wifi"))

    def test_toggle_button_with_icon(self) -> None:
        self._assert_widget("toggle-button-with-icon", ToggleButton(id="wifi", icon="network-wireless-symbolic"))

    def test_checkbox_on(self) -> None:
        self._assert_widget(
            "checkbox-on", Checkbox(id="autostart", label="Run at login", active=True)
        )

    def test_slider(self) -> None:
        self._assert_widget(
            "slider", Slider(id="brightness", min=0.0, max=1.0, step=0.05, value=0.6)
        )

    def test_select(self) -> None:
        self._assert_widget(
            "select",
            Select(
                id="env",
                items=[
                    {"id": "prod", "label": "Production"},
                    {"id": "stage", "label": "Staging"},
                ],
                selected=0,
            ),
        )

    def test_select_empty(self) -> None:
        self._assert_widget("select-empty", Select(id="env"))

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
            Hero(title="VPN", subtitle="Connected", icon="network-vpn-symbolic"),
        )

    def test_hero_with_switch(self) -> None:
        self._assert_widget(
            "hero-with-switch",
            Hero(title="VPN", subtitle="Connected", id="vpn-toggle", switch=True),
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
        self._assert_widget("status-dot-warning", StatusDot(variant=StatusVariant.WARNING))

    def test_pager_item_number_active(self) -> None:
        self._assert_widget(
            "pager-item-number-active",
            PagerItem(
                id="workspace-1",
                appearance=PagerAppearance.NUMBERS,
                label="1",
                active=True,
            ),
        )

    def test_pager_strip(self) -> None:
        self._assert_widget(
            "pager-strip",
            PagerStrip(
                items=[
                    PagerItem(
                        id="workspace-1",
                        appearance=PagerAppearance.NUMBERS,
                        label="1",
                        active=True,
                    ),
                    PagerItem(
                        id="workspace-2",
                        appearance=PagerAppearance.NUMBERS,
                        label="2",
                        occupied=True,
                    ),
                    PagerItem(
                        id="workspace-3",
                        appearance=PagerAppearance.DOTS,
                        urgent=True,
                    ),
                ]
            ),
        )

    def test_icon_by_name(self) -> None:
        self._assert_widget("icon-by-name", Icon(icon="user-info-symbolic"))

    def test_picture(self) -> None:
        self._assert_widget("picture", Picture(path="/home/me/photo.png"))

    def test_picture_content_fit(self) -> None:
        self._assert_widget(
            "picture-content-fit",
            Picture(path="/home/me/photo.png", content_fit="cover"),
        )

    def test_separator(self) -> None:
        self._assert_widget("separator", Separator())

    def test_row(self) -> None:
        self._assert_widget("row", Row())

    def test_column(self) -> None:
        self._assert_widget("column", Column())

    def test_grid(self) -> None:
        self._assert_widget(
            "grid",
            Grid(
                children=[
                    GridChild(row=0, column=0, child=Text(text="A")),
                    GridChild(row=0, column=1, width=2, child=Text(text="B")),
                ],
            ),
        )

    def test_scroll(self) -> None:
        self._assert_widget("scroll", Scroll(child=Text(text="scrollable")))

    def test_card(self) -> None:
        self._assert_widget("card", Card(child=Text(text="in card")))

    def test_card_empty(self) -> None:
        self._assert_widget("card-empty", Card())

    def test_container_styled(self) -> None:
        self._assert_widget(
            "container-styled",
            Container(
                width=220,
                height=80,
                min_width=180,
                min_height=48,
                margin=Space.XS,
                margin_top=Space.SM,
                padding=Space.MD,
                padding_left=Space.LG,
                background=Color.SURFACE_RAISED,
                color=Color.FG,
                border_radius=Radius.MD,
                border_width=BorderWidth.THIN,
                border_color=Color.BORDER,
                font_size=FontSize.SM,
                font_weight=FontWeight.SEMIBOLD,
                child=Text(text="contained"),
            ),
        )

    def test_property_list(self) -> None:
        self._assert_widget(
            "property-list",
            PropertyList({"IPv4": "10.0.0.42", "SSID": "home-5G"}),
        )

    def test_property_list_title(self) -> None:
        self._assert_widget(
            "property-list-title",
            PropertyList({"IPv4": "10.0.0.42", "SSID": "home-5G"}, title="Network"),
        )

    def test_property_list_empty(self) -> None:
        self._assert_widget("property-list-empty", PropertyList())

    def test_item(self) -> None:
        self._assert_widget("item", Item(label="Wi-Fi"))

    def test_item_with_right(self) -> None:
        self._assert_widget(
            "item-with-right",
            Item(
                icon="network-wireless-symbolic",
                label="Wi-Fi",
                sublabel="Connected",
                right=Badge(label="home-5G"),
            ),
        )

    def test_action_item(self) -> None:
        self._assert_widget("action-item", ActionItem(id="wifi", label="Wi-Fi"))

    def test_action_item_with_right(self) -> None:
        self._assert_widget(
            "action-item-with-right",
            ActionItem(
                id="wifi",
                icon="network-wireless-symbolic",
                label="Wi-Fi",
                sublabel="Connected",
                right=Badge(label="home-5G"),
            ),
        )

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
                id="volume",
                icon="audio-volume-medium-symbolic",
                label="Volume",
                value=0.42,
                max=1.0,
                text="42%",
                interactive=True,
            ),
        )

    def test_copyable(self) -> None:
        self._assert_widget("copyable", Copyable(label="IPv4", value="10.0.0.42"))

    def test_common_props_all(self) -> None:
        self._assert_widget(
            "common-props-all",
            Text(
                text="marked",
                visible=False,
                hexpand=True,
                vexpand=True,
                halign=Align.CENTER,
                valign=Align.END,
                tooltip="details",
                css_classes=["marked"],
                styles={"font-weight": "600", "margin-top": "2px"},
            ),
        )

    def test_tree_hero_column_card(self) -> None:
        self._assert_widget(
            "tree-hero-column-card",
            Column(
                children=[
                    Hero(title="Counter", subtitle="Value: 0"),
                    Card(
                        child=Column(
                            children=[
                                Text(text="Current"),
                                Button(id="increment", label="Increment"),
                            ]
                        ),
                    ),
                ],
            ),
        )

    def test_tree_card_with_grid(self) -> None:
        self._assert_widget(
            "tree-card-with-grid",
            Card(
                child=Grid(
                    row_spacing=4,
                    column_spacing=8,
                    children=[
                        GridChild(row=0, column=0, child=Text(text="K")),
                        GridChild(row=0, column=1, child=Badge(label="V")),
                    ],
                )
            ),
        )

    def test_popover_scaffold_basic(self) -> None:
        self._assert_widget(
            "popover-scaffold-basic",
            PopoverScaffold(body=Text(text="Content")),
        )

    def test_popover_scaffold_with_hero(self) -> None:
        self._assert_widget(
            "popover-scaffold-with-hero",
            PopoverScaffold(
                body=Text(text="Content"),
                hero=Hero(title="VPN", subtitle="Connected"),
                size=PopoverSize.LARGE,
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
