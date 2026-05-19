//! Golden cross-SDK fixture tests.
//!
//! Each test case builds a widget and asserts its JSON serialization equals
//! the corresponding fixture file under ../fixtures/widgets/.
//! Each event test parses the canonical incoming payload and asserts the
//! parser returns the documented typed event.

use std::fs;
use std::path::PathBuf;

use glimpse_sdk::{
    ActionItem, Align, Badge, BorderWidth, Button, ButtonVariant, CallbackEvent, Card, Checkbox,
    Color, Column, Container, ContentFit, Copyable, EmptyState, Expander, FontSize, FontWeight,
    Grid, GridChild, Hero, Icon, Item, LevelBar, LevelBarMode, LinkButton, Meter, PagerItem,
    PagerStrip, Picture, PopoverScaffold, PopoverSize, Progress, PropertyList, Radius, Row, Scroll,
    Select, Separator, Slider, Space, Spinner, StatusDot, StatusVariant, Switch, Text, TextAlign,
    ToggleButton, TreeNode, Variant, parse_callback_event,
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
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()))
}

#[track_caller]
fn assert_widget(name: &str, node: TreeNode<()>) {
    let expected = load(&format!("widgets/{name}.json"));
    let got = serde_json::to_value(&node).expect("serialize");
    assert_eq!(got, expected, "fixture mismatch for widgets/{name}.json");
}

#[test]
fn widget_text_styled() {
    assert_widget(
        "text-styled",
        Text::new("Aligned text")
            .color(Color::Accent)
            .size(FontSize::Lg)
            .weight(FontWeight::Bold)
            .align(TextAlign::Center)
            .into(),
    );
}

#[test]
fn widget_button_basic() {
    assert_widget("button-basic", Button::new("go").label("Go").into());
}

#[test]
fn widget_button_with_icon() {
    let mut b = Button::new("go").label("Go");
    b.icon = Some("go-symbolic".into());
    assert_widget("button-with-icon", b.into());
}

#[test]
fn widget_button_icon_only() {
    let mut b = Button::new("go");
    b.icon = Some("go-symbolic".into());
    assert_widget("button-icon-only", b.into());
}

#[test]
fn widget_button_primary() {
    assert_widget(
        "button-primary",
        Button::new("go")
            .label("Go")
            .variant(ButtonVariant::Primary)
            .into(),
    );
}

#[test]
fn widget_button_disabled() {
    assert_widget(
        "button-disabled",
        Button::new("go").label("Go").enabled(false).into(),
    );
}

#[test]
fn widget_link_button() {
    assert_widget("link-button", LinkButton::new("https://example.com").into());
}

#[test]
fn widget_link_button_label() {
    assert_widget(
        "link-button-label",
        LinkButton::new("https://example.com/docs")
            .label("Docs")
            .into(),
    );
}

#[test]
fn widget_expander() {
    assert_widget(
        "expander",
        Expander::new("Details").child(Text::new("More")).into(),
    );
}

#[test]
fn widget_expander_expanded() {
    assert_widget(
        "expander-expanded",
        Expander::new("Details")
            .expanded(true)
            .child(Text::new("More"))
            .into(),
    );
}

#[test]
fn widget_level_bar() {
    assert_widget(
        "level-bar",
        LevelBar::new(0.7)
            .min(0.0)
            .max(1.0)
            .mode(LevelBarMode::Continuous)
            .into(),
    );
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
fn widget_toggle_button_on() {
    let mut toggle = ToggleButton::new("wifi");
    toggle.label = Some("Wi-Fi".into());
    toggle.active = true;
    assert_widget("toggle-button-on", toggle.into());
}

#[test]
fn widget_toggle_button_off() {
    assert_widget("toggle-button-off", ToggleButton::new("wifi").into());
}

#[test]
fn widget_toggle_button_with_icon() {
    assert_widget(
        "toggle-button-with-icon",
        ToggleButton::new("wifi")
            .icon("network-wireless-symbolic")
            .into(),
    );
}

#[test]
fn widget_checkbox_on() {
    let mut c = Checkbox::new("autostart");
    c.label = Some("Run at login".into());
    c.active = true;
    assert_widget("checkbox-on", c.into());
}

#[test]
fn widget_slider() {
    let mut s = Slider::new("brightness");
    s.min = 0.0;
    s.max = 1.0;
    s.step = 0.05;
    s.value = 0.6;
    assert_widget("slider", s.into());
}

#[test]
fn widget_select() {
    let mut d = Select::new(
        "env",
        vec![
            ("prod".into(), "Production".into()),
            ("stage".into(), "Staging".into()),
        ],
    );
    d.selected = Some(0);
    assert_widget("select", d.into());
}

#[test]
fn widget_select_empty() {
    assert_widget("select-empty", Select::new("env", vec![]).into());
}

#[test]
fn widget_badge() {
    assert_widget("badge", Badge::new("42%").into());
}

#[test]
fn widget_badge_success_variant() {
    assert_widget(
        "badge-success-variant",
        Badge::new("OK").variant(Variant::Success).into(),
    );
}

#[test]
fn widget_hero_basic() {
    assert_widget("hero-basic", Hero::new("Counter", "Value: 0").into());
}

#[test]
fn widget_hero_with_icon() {
    assert_widget(
        "hero-with-icon",
        Hero::new("VPN", "Connected")
            .icon("network-vpn-symbolic")
            .into(),
    );
}

#[test]
fn widget_hero_with_switch() {
    assert_widget(
        "hero-with-switch",
        Hero::new("VPN", "Connected")
            .id("vpn-toggle")
            .switch(true)
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
        Progress::new(0.7)
            .max(1.0)
            .show_text(true)
            .text("70%")
            .into(),
    );
}

#[test]
fn widget_spinner_default() {
    assert_widget("spinner-default", Spinner::new().into());
}

#[test]
fn widget_spinner_stopped() {
    assert_widget("spinner-stopped", Spinner::new().spinning(false).into());
}

#[test]
fn widget_status_dot() {
    assert_widget("status-dot", StatusDot::new().into());
}

#[test]
fn widget_status_dot_warning() {
    assert_widget(
        "status-dot-warning",
        StatusDot::new().variant(StatusVariant::Warning).into(),
    );
}

#[test]
fn widget_pager_item_number_active() {
    assert_widget(
        "pager-item-number-active",
        PagerItem::number("1").id("workspace-1").active(true).into(),
    );
}

#[test]
fn widget_pager_strip() {
    assert_widget(
        "pager-strip",
        PagerStrip::new(vec![
            PagerItem::number("1").id("workspace-1").active(true),
            PagerItem::number("2").id("workspace-2").occupied(true),
            PagerItem::dots().id("workspace-3").urgent(true),
        ])
        .into(),
    );
}

#[test]
fn widget_icon_by_name() {
    assert_widget("icon-by-name", Icon::new("user-info-symbolic").into());
}

#[test]
fn widget_picture() {
    assert_widget("picture", Picture::new("/home/me/photo.png").into());
}

#[test]
fn widget_picture_content_fit() {
    assert_widget(
        "picture-content-fit",
        Picture::new("/home/me/photo.png")
            .content_fit(ContentFit::Cover)
            .into(),
    );
}

#[test]
fn widget_separator() {
    assert_widget("separator", Separator::new().into());
}

#[test]
fn widget_row() {
    assert_widget("row", Row::new(vec![]).into());
}

#[test]
fn widget_column() {
    assert_widget("column", Column::new(vec![]).into());
}

#[test]
fn widget_grid() {
    let grid = Grid::new(vec![GridChild::new(0, 0, Text::new("A").into()), {
        let mut c = GridChild::new(0, 1, Text::new("B").into());
        c.width = 2;
        c
    }]);
    assert_widget("grid", grid.into());
}

#[test]
fn widget_scroll() {
    assert_widget("scroll", Scroll::new(Text::new("scrollable").into()).into());
}

#[test]
fn widget_card() {
    assert_widget("card", Card::new(Some(Text::new("in card").into())).into());
}

#[test]
fn widget_card_empty() {
    assert_widget("card-empty", Card::new(None).into());
}

#[test]
fn widget_container_styled() {
    assert_widget(
        "container-styled",
        Container::new(Some(Text::new("contained").into()))
            .width(220)
            .height(80)
            .min_width(180)
            .min_height(48)
            .margin(Space::Xs)
            .margin_top(Space::Sm)
            .padding(Space::Md)
            .padding_left(Space::Lg)
            .background(Color::SurfaceRaised)
            .color(Color::Fg)
            .border_radius(Radius::Md)
            .border_width(BorderWidth::Thin)
            .border_color(Color::Border)
            .font_size(FontSize::Sm)
            .font_weight(FontWeight::Semibold)
            .into(),
    );
}

#[test]
fn widget_property_list() {
    assert_widget(
        "property-list",
        PropertyList::new([("IPv4", "10.0.0.42"), ("SSID", "home-5G")]).into(),
    );
}

#[test]
fn widget_property_list_title() {
    assert_widget(
        "property-list-title",
        PropertyList::new([("IPv4", "10.0.0.42"), ("SSID", "home-5G")])
            .title("Network")
            .into(),
    );
}

#[test]
fn widget_property_list_empty() {
    assert_widget("property-list-empty", PropertyList::default().into());
}

#[test]
fn widget_item() {
    assert_widget("item", Item::new("Wi-Fi").into());
}

#[test]
fn widget_item_with_right() {
    assert_widget(
        "item-with-right",
        Item::new("Wi-Fi")
            .icon("network-wireless-symbolic")
            .sublabel("Connected")
            .right(Badge::new("home-5G"))
            .into(),
    );
}

#[test]
fn widget_action_item() {
    assert_widget("action-item", ActionItem::new("wifi", "Wi-Fi").into());
}

#[test]
fn widget_action_item_with_right() {
    assert_widget(
        "action-item-with-right",
        ActionItem::new("wifi", "Wi-Fi")
            .icon("network-wireless-symbolic")
            .sublabel("Connected")
            .right(Badge::new("home-5G"))
            .into(),
    );
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
    assert_widget("meter", Meter::new("Memory", 0.51, 1.0).into());
}

#[test]
fn widget_meter_interactive() {
    let mut m = Meter::new("Volume", 0.42, 1.0).id("volume");
    m.icon = Some("audio-volume-medium-symbolic".into());
    m.text = Some("42%".into());
    m.interactive = true;
    assert_widget("meter-interactive", m.into());
}

#[test]
fn widget_copyable() {
    assert_widget("copyable", Copyable::new("IPv4", "10.0.0.42").into());
}

#[test]
fn widget_common_props_all() {
    let mut text = Text::new("marked");
    text.common.visible = Some(false);
    text.common.hexpand = Some(true);
    text.common.vexpand = Some(true);
    text.common.halign = Some(Align::Center);
    text.common.valign = Some(Align::End);
    text.common.tooltip = Some("details".into());
    text.common.css_classes.push("marked".into());
    text.common
        .styles
        .insert("font-weight".into(), "600".into());
    text.common.styles.insert("margin-top".into(), "2px".into());
    assert_widget("common-props-all", text.into());
}

#[test]
fn widget_tree_hero_column_card() {
    use glimpse_sdk::tree;
    let column = Column::new(tree![
        Hero::new("Counter", "Value: 0"),
        Card::new(Some(
            Column::new(tree![
                Text::new("Current"),
                Button::new("increment").label("Increment"),
            ])
            .into(),
        )),
    ]);
    assert_widget("tree-hero-column-card", column.into());
}

#[test]
fn widget_tree_card_with_grid() {
    let grid = {
        let mut g = Grid::new(vec![
            GridChild::new(0, 0, Text::new("K").into()),
            GridChild::new(0, 1, Badge::new("V").into()),
        ]);
        g.row_spacing = 4;
        g.column_spacing = 8;
        g
    };
    let card = Card::new(Some(grid.into()));
    assert_widget("tree-card-with-grid", card.into());
}

#[test]
fn widget_popover_scaffold_basic() {
    let scaffold = PopoverScaffold::new(Text::new("Content"));
    assert_widget("popover-scaffold-basic", scaffold.into());
}

#[test]
fn widget_popover_scaffold_with_hero() {
    let scaffold = PopoverScaffold::new(Text::new("Content"))
        .hero(Hero::new("VPN", "Connected"))
        .size(PopoverSize::Large);
    assert_widget("popover-scaffold-with-hero", scaffold.into());
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
        assert_eq!(
            c.button.as_deref().unwrap(),
            parsed["button"].as_str().unwrap()
        );
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
