//! Golden cross-SDK fixture tests.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use glimpse_sdk::{
    ActiveIndicator, Badge, BadgeKind, BatteryHero, BoxedList, ButtonRow, Calendar, CallbackEvent,
    CameraIndicator, Choice, ChoiceList, ChoiceTile, Column, Container, DateHero, EmptyState,
    EventItem, Events, ExpanderTile, Header, Hero, KeyValueGrid, KeyValueRow, Label,
    LocationIndicator, Meter, MicIndicator, MutedIndicator, PagerAppearance, PagerItem, PagerStrip,
    PopoverShell, PopoverSize, Row, ScreenCastIndicator, Scroll, Separator, SliderTile, Spinner,
    StatusDot, StatusDotStatus, SwitchTile, Tile, TreeNode, WeatherForecastItem,
    WeatherForecastList, WeatherHourlyItem, WeatherHourlyStrip, WorldClock, WorldClockRow,
    parse_callback_event,
};
use serde_json::{Value, json};

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

fn label() -> Label {
    let mut label = Label::new("Ready");
    label.wrap = Some(true);
    label
}

fn badge() -> Badge {
    let mut badge = Badge::new("OK");
    badge.kind = BadgeKind::Success;
    badge
}

fn key_value(key: &str, value: &str) -> KeyValueRow {
    KeyValueRow {
        key: key.into(),
        value: value.into(),
    }
}

fn widgets() -> BTreeMap<&'static str, TreeNode<()>> {
    let mut out = BTreeMap::new();

    out.insert("label", label().into());
    out.insert("header", Header::new("Network").into());
    let mut hero = Hero::new("VPN", "Disconnected");
    hero.id = Some("vpn".into());
    hero.icon = Some("network-vpn-symbolic".into());
    hero.icon_size = Some(32);
    hero.toggle = Some(false);
    hero.toggle_sensitive = Some(true);
    hero.separator = Some(true);
    hero.trailing = Some(Box::new(badge().into()));
    out.insert("hero", hero.into());
    out.insert("badge", badge().into());
    let mut dot = StatusDot::new();
    dot.status = StatusDotStatus::Warning;
    out.insert("status-dot", dot.clone().into());

    let mut indicator = glimpse_sdk::PanelIndicator::new();
    indicator.id = Some("net".into());
    indicator.icon = Some("network-wireless-symbolic".into());
    indicator.label = Some("Wi-Fi".into());
    indicator.active = true;
    indicator.extra = Some(Box::new(dot.into()));
    out.insert("panel-indicator", indicator.into());
    let mut empty = EmptyState::new("No devices");
    empty.subtitle = Some("Connect a device to continue".into());
    out.insert("empty-state", empty.into());

    out.insert("spinner", Spinner::new().into());
    out.insert("meter", Meter::new("Memory", 0.51).into());
    out.insert("separator", Separator::new().into());
    out.insert("scroll", Scroll::new(label().into()).into());
    out.insert("row", Row::new(vec![label().into(), badge().into()]).into());
    out.insert(
        "column",
        Column::new(vec![label().into(), badge().into()]).into(),
    );
    out.insert(
        "boxed-list",
        BoxedList::new(vec![label().into(), badge().into()]).into(),
    );
    out.insert(
        "button-row",
        ButtonRow::new(vec![Tile::new("Refresh").into()]).into(),
    );

    out.insert("container", Container::new(vec![label().into()]).into());

    let mut shell = PopoverShell::new(vec![label().into()]);
    shell.footer = vec![badge().into()];
    shell.footer_visible = true;
    out.insert("popover-shell", shell.into());

    let mut tile = Tile::new("Wi-Fi");
    tile.id = Some("wifi".into());
    tile.secondary = Some("Connected".into());
    tile.left_icon = Some("network-wireless-symbolic".into());
    tile.right = Some(Box::new(badge().into()));
    out.insert("tile", tile.into());

    let mut segmented = glimpse_sdk::SegmentedTile::new("Backup");
    segmented.id = Some("drive".into());
    segmented.secondary = Some("Mounted".into());
    segmented.left_icon = Some("drive-harddisk-symbolic".into());
    segmented.right = Some(Box::new(badge().into()));
    segmented.child = Some(Box::new(
        KeyValueGrid::new(vec![key_value("Size", "1 TB")]).into(),
    ));
    segmented.expanded = true;
    out.insert("segmented-tile", segmented.into());

    let mut switch = SwitchTile::new("bluetooth", "Bluetooth");
    switch.secondary = Some("On".into());
    switch.left_icon = Some("bluetooth-active-symbolic".into());
    switch.active = true;
    out.insert("switch-tile", switch.into());

    let mut expander = ExpanderTile::new("Details");
    expander.id = Some("details".into());
    expander.secondary = Some("2 items".into());
    expander.left_icon = Some("view-list-symbolic".into());
    expander.child = Some(Box::new(Column::new(vec![label().into()]).into()));
    expander.expanded = true;
    out.insert("expander-tile", expander.into());

    let mut slider = SliderTile::new("brightness");
    slider.label = Some("Brightness".into());
    slider.left_icon = Some("display-brightness-symbolic".into());
    slider.value = 0.6;
    slider.step = 0.05;
    slider.snap_step = Some(0.05);
    out.insert("slider-tile", slider.into());

    let mut choice_tile = ChoiceTile::new("Balanced");
    choice_tile.id = Some("balanced".into());
    choice_tile.secondary = Some("Recommended".into());
    choice_tile.left_icon = Some("power-profile-balanced-symbolic".into());
    choice_tile.selected = true;
    out.insert("choice-tile", choice_tile.into());

    out.insert(
        "choice-list",
        ChoiceList::new(
            "profile",
            vec![
                Choice {
                    id: "balanced".into(),
                    primary: "Balanced".into(),
                    secondary: Some("Recommended".into()),
                    icon: Some("power-profile-balanced-symbolic".into()),
                },
                Choice {
                    id: "performance".into(),
                    primary: "Performance".into(),
                    secondary: Some("Fast".into()),
                    icon: Some("power-profile-performance-symbolic".into()),
                },
            ],
        )
        .tap(|list| list.active = Some("balanced".into()))
        .into(),
    );
    out.insert(
        "key-value-grid",
        KeyValueGrid::new(vec![key_value("IPv4", "10.0.0.42")]).into(),
    );

    let mut pager_item = PagerItem::new(1);
    pager_item.label = "1".into();
    pager_item.appearance = PagerAppearance::Numbers;
    pager_item.active = true;
    pager_item.occupied = true;
    out.insert("pager-item", pager_item.clone().into());
    let mut pager_two = PagerItem::new(2);
    pager_two.label = "2".into();
    pager_two.appearance = PagerAppearance::Numbers;
    pager_two.inactive = true;
    let mut pager = PagerStrip::new(vec![pager_item, pager_two]);
    pager.id = Some("workspaces".into());
    out.insert("pager-strip", pager.into());

    out.insert(
        "camera-indicator",
        CameraIndicator {
            data: ActiveIndicator::active(),
        }
        .into(),
    );
    out.insert(
        "mic-indicator",
        MicIndicator {
            data: ActiveIndicator::active(),
        }
        .into(),
    );
    out.insert(
        "muted-indicator",
        MutedIndicator {
            data: ActiveIndicator::active(),
        }
        .into(),
    );
    out.insert(
        "location-indicator",
        LocationIndicator {
            data: ActiveIndicator::active(),
        }
        .into(),
    );
    out.insert(
        "screencast-indicator",
        ScreenCastIndicator {
            data: ActiveIndicator::active(),
            timer_text: Some("01:23".into()),
        }
        .into(),
    );

    out.insert(
        "calendar",
        Calendar {
            common: Default::default(),
            id: Some("calendar".into()),
            selected_date: "2026-05-22".into(),
            event_days: vec!["2026-05-22".into(), "2026-05-24".into()],
            on_change: None,
        }
        .into(),
    );
    out.insert(
        "battery-hero",
        BatteryHero {
            common: Default::default(),
            icon: "battery-good-symbolic".into(),
            percentage: "82%".into(),
            fraction: 0.82,
            state: "Discharging".into(),
        }
        .into(),
    );
    out.insert(
        "date-hero",
        DateHero {
            common: Default::default(),
            weekday: "Friday".into(),
            date: "May 22".into(),
        }
        .into(),
    );
    out.insert(
        "events",
        Events {
            common: Default::default(),
            date: "2026-05-22".into(),
            events: vec![EventItem {
                id: "standup".into(),
                title: "Standup".into(),
                start: "09:30".into(),
                end: "09:45".into(),
                location: None,
                all_day: false,
            }],
            loading: false,
        }
        .into(),
    );
    out.insert(
        "weather-forecast-list",
        WeatherForecastList {
            common: Default::default(),
            items: vec![WeatherForecastItem {
                day_name: "Today".into(),
                icon: "weather-clear-symbolic".into(),
                condition: "Clear".into(),
                temperatures: "12 / 20".into(),
                is_today: true,
            }],
        }
        .into(),
    );
    out.insert(
        "weather-hourly-strip",
        WeatherHourlyStrip {
            common: Default::default(),
            items: vec![WeatherHourlyItem {
                time: "12:00".into(),
                icon: "weather-clear-symbolic".into(),
                temperature: "18".into(),
            }],
        }
        .into(),
    );
    out.insert(
        "world-clock",
        WorldClock {
            common: Default::default(),
            rows: vec![WorldClockRow {
                name: "UTC".into(),
                timezone: "UTC".into(),
                time: "12:00".into(),
                offset: "+00:00".into(),
                day_label: Some("Today".into()),
            }],
        }
        .into(),
    );

    let mut shared = PopoverShell::new(vec![
        Hero::new("System", "Shared widgets").into(),
        BoxedList::new(vec![{
            let mut wifi = SwitchTile::new("wifi", "Wi-Fi");
            wifi.active = true;
            wifi.into()
        }])
        .into(),
    ]);
    shared.size = PopoverSize::Large;
    out.insert("tree-shared-popover", shared.into());

    out
}

trait Tap: Sized {
    fn tap(mut self, f: impl FnOnce(&mut Self)) -> Self {
        f(&mut self);
        self
    }
}
impl<T> Tap for T {}

#[test]
fn widgets_match_fixtures() {
    for (name, node) in widgets() {
        let expected = load(&format!("widgets/{name}.json"));
        let got = serde_json::to_value(&node).expect("serialize");
        assert_eq!(got, expected, "fixture mismatch for widgets/{name}.json");
    }
}

#[test]
fn no_removed_widgets_are_exposed() {
    let public = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("read lib.rs");
    let tokens: Vec<&str> = public
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .collect();
    for removed in [
        "Button",
        "ActionItem",
        "Card",
        "PropertyList",
        "Item",
        "PopoverScaffold",
        "AnimatedPopover",
        "Message",
        "MessageGroup",
        "MediaArtwork",
        "MediaMeta",
        "MediaScrubber",
        "MediaTransport",
        "NowPlayingCard",
        "SecondaryPlayerRow",
    ] {
        assert!(!tokens.contains(&removed), "{removed} must not be exported");
    }
}

fn load_event(name: &str) -> (Value, Value) {
    let raw = load(&format!("events/{name}.json"));
    (raw["incoming"].clone(), raw["parsed"].clone())
}

#[test]
fn events_match_fixtures() {
    for name in [
        "click-left",
        "click-no-button",
        "scroll-down",
        "input",
        "toggle-active-true",
        "toggle-active-false",
        "toggle-via-value-true",
        "toggle-numeric-value-is-false",
        "change-scale",
        "change-dropdown",
        "popover-open",
        "popover-close",
    ] {
        let (incoming, parsed) = load_event(name);
        let got = match parse_callback_event(incoming).expect("parse") {
            CallbackEvent::Click(e) => json!({ "id": e.id, "event": "click", "button": e.button }),
            CallbackEvent::Scroll(e) => {
                json!({ "id": e.id, "event": "scroll", "delta_y": e.delta_y })
            }
            CallbackEvent::Input(e) => json!({ "id": e.id, "event": "input", "text": e.text }),
            CallbackEvent::Toggle(e) => json!({ "id": e.id, "event": "toggle", "value": e.value }),
            CallbackEvent::Change(e) => json!({ "id": e.id, "event": "change", "value": e.value }),
            CallbackEvent::Popover(e) => {
                json!({ "id": "popover", "event": if e.open { "open" } else { "close" }, "open": e.open })
            }
        };
        assert_eq!(got, parsed, "event fixture mismatch for events/{name}.json");
    }
}
