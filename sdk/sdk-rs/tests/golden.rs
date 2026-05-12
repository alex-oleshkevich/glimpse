//! Golden cross-SDK fixture tests.
//!
//! Each test case builds a widget and asserts its JSON serialization equals
//! the corresponding fixture file under ../fixtures/widgets/.
//! Each event test parses the canonical incoming payload and asserts the
//! parser returns the documented typed event.

use std::fs;
use std::path::PathBuf;

use glimpse_sdk::{
    ActionMenu, ActionMenuItem, ActionRow, Align, Badge, BoxNode, Button, Card, CallbackEvent,
    Checkbox, Collapsible, CollapsibleItem, Column, Copyable, DetailGrid, DetailGridItem, Dropdown,
    DropdownItem, EmptyState, Grid, GridChild, Hero, Icon, IconWidget, Image, Item, Label, MenuItem,
    Meter, Progress, Row, Scale, Scroll, Section, Separator, Spinner, StatusDot, Switch, Toast,
    ToastAction, TreeNode, Variant, parse_callback_event,
};
use serde_json::Value;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
}

fn load(rel: &str) -> Value {
    let path = fixtures_root().join(rel);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()))
}

#[track_caller]
fn assert_widget(name: &str, node: TreeNode) {
    let expected = load(&format!("widgets/{name}.json"));
    let got = serde_json::to_value(&node).expect("serialize");
    assert_eq!(got, expected, "fixture mismatch for widgets/{name}.json");
}

#[test]
fn widget_label_basic() {
    assert_widget("label-basic", Label::new("Hello").into());
}

#[test]
fn widget_label_modifiers() {
    let mut label = Label::new("Hello");
    label.wrap = true;
    label.xalign = Some(0.5);
    label.selectable = true;
    assert_widget("label-modifiers", label.into());
}

#[test]
fn widget_button_basic() {
    assert_widget("button-basic", Button::new("go").label("Go").into());
}

#[test]
fn widget_button_with_icon() {
    let mut b = Button::new("go").label("Go");
    b.icon = Some(Icon::name("go-symbolic"));
    assert_widget("button-with-icon", b.into());
}

#[test]
fn widget_button_icon_only() {
    let mut b = Button::new("go");
    b.icon = Some(Icon::name("go-symbolic"));
    assert_widget("button-icon-only", b.into());
}

#[test]
fn widget_switch_on() {
    let mut s = Switch::new("vpn");
    s.label = Some("VPN".into());
    s.active = true;
    assert_widget("switch-on", s.into());
}

#[test]
fn widget_switch_off() {
    assert_widget("switch-off", Switch::new("vpn").into());
}

#[test]
fn widget_checkbox_on() {
    let mut c = Checkbox::new("autostart");
    c.label = Some("Run at login".into());
    c.active = true;
    assert_widget("checkbox-on", c.into());
}

#[test]
fn widget_scale() {
    let mut s = Scale::new("brightness");
    s.min = 0.0;
    s.max = 1.0;
    s.step = 0.05;
    s.value = 0.6;
    assert_widget("scale", s.into());
}

#[test]
fn widget_dropdown() {
    let mut d = Dropdown::new(
        "env",
        vec![
            DropdownItem::new("prod", "Production"),
            DropdownItem::new("stage", "Staging"),
        ],
    );
    d.selected = Some(0);
    assert_widget("dropdown", d.into());
}

#[test]
fn widget_dropdown_empty() {
    assert_widget("dropdown-empty", Dropdown::new("env", vec![]).into());
}

#[test]
fn widget_badge() {
    assert_widget("badge", Badge::new("42%").into());
}

#[test]
fn widget_badge_success_variant() {
    let mut b = Badge::new("OK");
    b.common.variant = Some(Variant::Success);
    assert_widget("badge-success-variant", b.into());
}

#[test]
fn widget_hero_basic() {
    assert_widget(
        "hero-basic",
        Hero::new("Counter", "Value: 0").into(),
    );
}

#[test]
fn widget_hero_with_icon() {
    assert_widget(
        "hero-with-icon",
        Hero::new("VPN", "Connected")
            .icon(Icon::name("network-vpn-symbolic"))
            .into(),
    );
}

#[test]
fn widget_progress() {
    assert_widget("progress", Progress::new(0.7).max(1.0).into());
}

#[test]
fn widget_progress_with_text() {
    assert_widget(
        "progress-with-text",
        Progress::new(0.7).max(1.0).show_text(true).text("70%").into(),
    );
}

#[test]
fn widget_spinner_default() {
    assert_widget("spinner-default", Spinner::new().into());
}

#[test]
fn widget_spinner_stopped() {
    assert_widget(
        "spinner-stopped",
        Spinner::new().spinning(false).into(),
    );
}

#[test]
fn widget_status_dot() {
    assert_widget("status-dot", StatusDot::new().into());
}

#[test]
fn widget_status_dot_warning() {
    let mut dot = StatusDot::new();
    dot.common.variant = Some(Variant::Warning);
    assert_widget("status-dot-warning", dot.into());
}

#[test]
fn widget_icon() {
    assert_widget(
        "icon",
        IconWidget::new(Icon::name("network-wireless-symbolic"))
            .pixel_size(24)
            .into(),
    );
}

#[test]
fn widget_image_by_name() {
    assert_widget(
        "image-by-name",
        Image::new(Icon::name("user-info-symbolic")).into(),
    );
}

#[test]
fn widget_image_by_path() {
    let mut img = Image::new(Icon::path("/home/me/avatar.png"));
    img.pixel_size = Some(64);
    assert_widget("image-by-path", img.into());
}

#[test]
fn widget_separator() {
    assert_widget("separator", Separator::new().into());
}

#[test]
fn widget_box_vertical() {
    assert_widget(
        "box-vertical",
        BoxNode::vertical(vec![]).spacing(8).into(),
    );
}

#[test]
fn widget_box_horizontal() {
    assert_widget(
        "box-horizontal",
        BoxNode::horizontal(vec![]).spacing(4).into(),
    );
}

#[test]
fn widget_row() {
    assert_widget("row", Row::new(vec![]).spacing(8).into());
}

#[test]
fn widget_column() {
    assert_widget("column", Column::new(vec![]).spacing(8).into());
}

#[test]
fn widget_grid() {
    let mut grid = Grid::new(vec![
        GridChild::new(0, 0, Label::new("A").into()),
        {
            let mut c = GridChild::new(0, 1, Label::new("B").into());
            c.width = 2;
            c
        },
    ]);
    grid.row_spacing = 4;
    grid.column_spacing = 4;
    assert_widget("grid", grid.into());
}

#[test]
fn widget_scroll() {
    assert_widget(
        "scroll",
        Scroll::new(Label::new("scrollable").into()).into(),
    );
}

#[test]
fn widget_card() {
    assert_widget(
        "card",
        Card::new(vec![Label::new("in card").into()]).into(),
    );
}

#[test]
fn widget_card_empty() {
    assert_widget("card-empty", Card::new(vec![]).into());
}

#[test]
fn widget_section_basic() {
    assert_widget(
        "section-basic",
        Section::new("System", vec![Label::new("uptime").into()]).into(),
    );
}

#[test]
fn widget_section_empty_body() {
    assert_widget(
        "section-empty-body",
        Section::new("Empty", vec![]).into(),
    );
}

#[test]
fn widget_collapsible_closed() {
    assert_widget(
        "collapsible-closed",
        Collapsible::new("Advanced", false, vec![]).into(),
    );
}

#[test]
fn widget_collapsible_open_with_body() {
    assert_widget(
        "collapsible-open-with-body",
        Collapsible::new("Advanced", true, vec![Label::new("inside").into()]).into(),
    );
}

#[test]
fn widget_item_basic() {
    assert_widget("item-basic", Item::new("Plain").into());
}

#[test]
fn widget_item_clickable() {
    assert_widget(
        "item-clickable",
        Item::clickable("run", "Run").into(),
    );
}

#[test]
fn widget_item_with_menu() {
    let item = Item::clickable("wifi-home", "home-5G").menu(vec![
        MenuItem::new("forget", "Forget"),
        MenuItem::new("details", "Details").enabled(false),
    ]);
    assert_widget("item-with-menu", item.into());
}

#[test]
fn widget_collapsible_item() {
    assert_widget(
        "collapsible-item",
        CollapsibleItem::new("Devices", false, vec![]).into(),
    );
}

#[test]
fn widget_action_row() {
    assert_widget(
        "action-row",
        ActionRow::new("go", "Connect").into(),
    );
}

#[test]
fn widget_action_row_with_meta() {
    assert_widget(
        "action-row-with-meta",
        ActionRow::new("go", "Connect")
            .subtitle("wg0")
            .meta("4 routes")
            .icon(Icon::name("network-vpn-symbolic"))
            .into(),
    );
}

#[test]
fn widget_action_menu() {
    assert_widget(
        "action-menu",
        ActionMenu::new(vec![
            ActionMenuItem::new("saver", "Power Saver").checked(false),
            ActionMenuItem::new("balanced", "Balanced").checked(true),
        ])
        .header("Power profile")
        .into(),
    );
}

#[test]
fn widget_action_menu_empty() {
    assert_widget(
        "action-menu-empty",
        ActionMenu::new(vec![]).into(),
    );
}

#[test]
fn widget_detail_grid() {
    assert_widget(
        "detail-grid",
        DetailGrid::new(vec![
            DetailGridItem::new("SSID", "home-5G"),
            DetailGridItem::new("IPv4", "10.0.0.42"),
        ])
        .into(),
    );
}

#[test]
fn widget_detail_grid_empty() {
    assert_widget("detail-grid-empty", DetailGrid::new(vec![]).into());
}

#[test]
fn widget_empty_state() {
    assert_widget("empty-state", EmptyState::new("Nothing here").into());
}

#[test]
fn widget_empty_state_with_subtitle() {
    assert_widget(
        "empty-state-with-subtitle",
        EmptyState::new("Nothing here")
            .subtitle("Plug in a device.")
            .into(),
    );
}

#[test]
fn widget_meter() {
    assert_widget(
        "meter",
        Meter::new("Memory", 0.51, 1.0).into(),
    );
}

#[test]
fn widget_meter_interactive() {
    let mut m = Meter::new("Volume", 0.42, 1.0);
    m.icon = Some(Icon::name("audio-volume-medium-symbolic"));
    m.text = Some("42%".into());
    m.interactive = true;
    assert_widget("meter-interactive", m.into());
}

#[test]
fn widget_copyable() {
    assert_widget(
        "copyable",
        Copyable::new("IPv4", "10.0.0.42").into(),
    );
}

#[test]
fn widget_toast() {
    assert_widget("toast", Toast::new("Saved", "").into());
}

#[test]
fn widget_toast_with_action() {
    let mut t = Toast::new("Update available", "Version 0.8 is available.");
    t.icon = Some(Icon::name("dialog-warning-symbolic"));
    t.action = Some(ToastAction {
        id: "update".into(),
        label: "Update".into(),
    });
    assert_widget("toast-with-action", t.into());
}

#[test]
fn widget_common_props_all() {
    let mut label = Label::new("marked");
    label.common.id = Some("marked".into());
    label.common.visible = Some(false);
    label.common.hexpand = Some(true);
    label.common.vexpand = Some(true);
    label.common.halign = Some(Align::Center);
    label.common.valign = Some(Align::End);
    label.common.tooltip = Some("details".into());
    label.common.variant = Some(Variant::Warning);
    assert_widget("common-props-all", label.into());
}

#[test]
fn widget_tree_hero_column_section() {
    let tree = Column::new(vec![
        TreeNode::from(Hero::new("Counter", "Value: 0")),
        TreeNode::from(Section::new(
            "Controls",
            vec![
                Label::new("Current").into(),
                Button::new("increment").label("Increment").into(),
            ],
        )),
    ])
    .spacing(8);
    assert_widget("tree-hero-column-section", tree.into());
}

#[test]
fn widget_tree_card_with_grid() {
    let grid = {
        let mut g = Grid::new(vec![
            GridChild::new(0, 0, Label::new("K").into()),
            GridChild::new(0, 1, Badge::new("V").into()),
        ]);
        g.row_spacing = 4;
        g.column_spacing = 8;
        g
    };
    let card = Card::new(vec![grid.into()]);
    assert_widget("tree-card-with-grid", card.into());
}

// ---------- events ----------

fn load_event(name: &str) -> (Value, Value) {
    let raw = load(&format!("events/{name}.json"));
    (raw["incoming"].clone(), raw["parsed"].clone())
}

#[track_caller]
fn assert_event(name: &str, check: impl FnOnce(CallbackEvent, &Value)) {
    let (incoming, parsed) = load_event(name);
    let event = parse_callback_event(incoming).expect("parse");
    check(event, &parsed);
}

#[test]
fn event_click_left() {
    assert_event("click-left", |e, parsed| {
        let CallbackEvent::Click(c) = e else {
            panic!("expected click event");
        };
        assert_eq!(c.id, parsed["id"].as_str().unwrap());
        assert_eq!(c.button.as_deref().unwrap(), parsed["button"].as_str().unwrap());
    });
}

#[test]
fn event_click_no_button() {
    assert_event("click-no-button", |e, parsed| {
        let CallbackEvent::Click(c) = e else {
            panic!("expected click event");
        };
        assert_eq!(c.id, parsed["id"].as_str().unwrap());
        assert!(c.button.is_none());
    });
}

#[test]
fn event_scroll() {
    assert_event("scroll-down", |e, parsed| {
        let CallbackEvent::Scroll(s) = e else {
            panic!("expected scroll");
        };
        assert_eq!(s.id, parsed["id"].as_str().unwrap());
        assert_eq!(s.delta_y, Some(parsed["delta_y"].as_f64().unwrap()));
    });
}

#[test]
fn event_input() {
    assert_event("input", |e, parsed| {
        let CallbackEvent::Input(i) = e else {
            panic!("expected input");
        };
        assert_eq!(i.id, parsed["id"].as_str().unwrap());
        assert_eq!(i.text, parsed["text"].as_str().unwrap());
    });
}

#[test]
fn event_toggle_active_true() {
    assert_event("toggle-active-true", |e, parsed| {
        let CallbackEvent::Toggle(t) = e else {
            panic!("expected toggle");
        };
        assert_eq!(t.value, parsed["value"].as_bool().unwrap());
    });
}

#[test]
fn event_toggle_active_false() {
    assert_event("toggle-active-false", |e, parsed| {
        let CallbackEvent::Toggle(t) = e else {
            panic!("expected toggle");
        };
        assert_eq!(t.value, parsed["value"].as_bool().unwrap());
    });
}

#[test]
fn event_toggle_via_value_true() {
    assert_event("toggle-via-value-true", |e, parsed| {
        let CallbackEvent::Toggle(t) = e else {
            panic!("expected toggle");
        };
        assert_eq!(t.value, parsed["value"].as_bool().unwrap());
    });
}

#[test]
fn event_toggle_numeric_value_is_false() {
    assert_event("toggle-numeric-value-is-false", |e, _parsed| {
        let CallbackEvent::Toggle(t) = e else {
            panic!("expected toggle");
        };
        assert_eq!(t.value, false);
    });
}

#[test]
fn event_change_scale() {
    assert_event("change-scale", |e, parsed| {
        let CallbackEvent::Change(c) = e else {
            panic!("expected change");
        };
        assert_eq!(c.id, parsed["id"].as_str().unwrap());
        assert_eq!(c.value, Some(parsed["value"].clone()));
    });
}

#[test]
fn event_change_dropdown() {
    assert_event("change-dropdown", |e, parsed| {
        let CallbackEvent::Change(c) = e else {
            panic!("expected change");
        };
        assert_eq!(c.id, parsed["id"].as_str().unwrap());
        assert_eq!(c.value, Some(parsed["value"].clone()));
    });
}

#[test]
fn event_popover_open() {
    assert_event("popover-open", |e, parsed| {
        let CallbackEvent::Popover(p) = e else {
            panic!("expected popover");
        };
        assert_eq!(p.open, parsed["open"].as_bool().unwrap());
    });
}

#[test]
fn event_popover_close() {
    assert_event("popover-close", |e, parsed| {
        let CallbackEvent::Popover(p) = e else {
            panic!("expected popover");
        };
        assert_eq!(p.open, parsed["open"].as_bool().unwrap());
    });
}
