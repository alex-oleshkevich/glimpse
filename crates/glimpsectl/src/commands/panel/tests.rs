use super::*;
use glimpse_dbus::testing::PrivateBus;

fn value(value: Value<'_>) -> OwnedValue {
    OwnedValue::try_from(value).unwrap()
}

fn row(connector: &str, position: Position, name: &str, own_view: bool) -> AppletRow {
    AppletRow {
        action: "applet.1".into(),
        connector: connector.into(),
        position,
        zone: "start".into(),
        index: 0,
        name: name.into(),
        kind: None,
        alive: true,
        own_view,
        popover: false,
        indicators: Vec::new(),
    }
}

#[test]
fn resolves_placements_and_rejects_own_view_item_input() {
    let rows = [
        row("DP-1", Position::Top, "clock", true),
        row("DP-1", Position::Bottom, "clock", false),
    ];
    assert_eq!(
        resolve(&rows, "DP-2", "clock", None).unwrap_err().0,
        "DP-2 has no applet `clock`"
    );
    assert_eq!(
        resolve(&rows, "DP-1", "clock", None).unwrap_err().0,
        "clock is on DP-1 top and bottom; pass --position"
    );
    assert_eq!(
        resolve(&rows, "DP-1", "clock", Some(Position::Bottom))
            .unwrap()
            .position,
        Position::Bottom
    );
    assert_eq!(
        target(&rows, "DP-1", "clock", Some(Position::Top), true)
            .unwrap_err()
            .0,
        "clock takes clicks per item; use `panel popover` instead"
    );
    assert!(target(&rows, "DP-1", "clock", Some(Position::Top), false).is_ok());
}

#[test]
fn rejects_duplicate_applet_in_one_position() {
    let mut second = row("DP-1", Position::Top, "clock", false);
    second.zone = "center".into();
    let rows = [row("DP-1", Position::Top, "clock", false), second];
    for position in [None, Some(Position::Top)] {
        assert_eq!(
            resolve(&rows, "DP-1", "clock", position).unwrap_err().0,
            "clock appears more than once on DP-1 top"
        );
    }
}

#[test]
fn removal_drops_the_applet_and_reports_its_serial() {
    let mut visible = vec![row("DP-1", Position::Top, "clock", false)];
    assert_eq!(
        removal_line(&mut visible, "applet.1", true).as_deref(),
        Some(r#"{"removed":"applet.1"}"#)
    );
    assert!(visible.is_empty());
    assert_eq!(
        removal_line(&mut visible, "applet.1", false).as_deref(),
        Some("1 removed")
    );
}

#[test]
fn decodes_state_and_sorts_by_placement() {
    let indicator = HashMap::from([
        ("icon".to_owned(), value(Value::from("weather-clear"))),
        ("label".to_owned(), value(Value::from("Sunny"))),
        ("attention".to_owned(), value(Value::from(true))),
    ]);
    let state = |zone: &str, index: u32| {
        value(Value::from(HashMap::from([
            ("connector".to_owned(), value(Value::from("DP-1"))),
            ("position".to_owned(), value(Value::from("top"))),
            ("zone".to_owned(), value(Value::from(zone))),
            ("index".to_owned(), value(Value::from(index))),
            ("name".to_owned(), value(Value::from("clock"))),
            ("kind".to_owned(), value(Value::from("builtin"))),
            ("alive".to_owned(), value(Value::from(true))),
            ("own-view".to_owned(), value(Value::from(false))),
            ("popover".to_owned(), value(Value::from(true))),
            (
                "indicators".to_owned(),
                value(Value::from(vec![indicator.clone()])),
            ),
        ])))
    };
    let mut rows = [
        decode("applet.2", &state("center", 0)).unwrap(),
        decode("applet.1", &state("start", 2)).unwrap(),
    ];
    sort(&mut rows);
    assert_eq!(rows[0].zone, "start");
    assert_eq!(rows[1].zone, "center");
    assert_eq!(rows[0].indicators[0].icon.as_deref(), Some("weather-clear"));
    assert_eq!(rows[0].indicators[0].label.as_deref(), Some("Sunny"));
    assert_eq!(rows[0].indicators[0].attention, Some(true));
    assert_eq!(rows[0].indicators[0].badge, None);
    let sparse = value(Value::from(HashMap::from([
        ("connector".to_owned(), value(Value::from("DP-1"))),
        ("position".to_owned(), value(Value::from("top"))),
        ("zone".to_owned(), value(Value::from("start"))),
        ("index".to_owned(), value(Value::from(0_u32))),
        ("name".to_owned(), value(Value::from("clock"))),
        ("kind".to_owned(), value(Value::from("builtin"))),
        ("alive".to_owned(), value(Value::from(true))),
        ("own-view".to_owned(), value(Value::from(false))),
        ("popover".to_owned(), value(Value::from(false))),
    ])));
    let sparse = decode("applet.3", &sparse).unwrap();
    assert_eq!(sparse.kind.as_deref(), Some("builtin"));
    assert!(sparse.indicators.is_empty());
}

#[tokio::test]
async fn missing_panel_is_unreachable() {
    let bus = PrivateBus::start();
    let client = bus.connection().await;
    let error = panel(
        &client,
        "me.aresa.GlimpsePanel",
        PanelCommand::Applets { watch: false },
        false,
    )
    .await
    .unwrap_err();
    assert_eq!(
        crate::errors::exit(&error),
        crate::errors::Exit::Unreachable
    );
}
