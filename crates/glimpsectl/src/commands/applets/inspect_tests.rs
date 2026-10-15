use super::*;

#[test]
fn config_formats_empty_and_placed_applets() {
    assert_eq!(config_text(&[]), "not placed");
    let place = Placement {
        instance: "clock\nmain".to_owned(),
        zone: "left".to_owned(),
        panel: 1,
        options: Map::from_iter([("label".to_owned(), Value::from("Work"))]),
    };
    assert_eq!(
        config_text(&[place]),
        "panel 1 · left · clock main\n  options: label = \"Work\""
    );
}

#[test]
fn unit_names_parse_back_to_applet_and_slot() {
    assert_eq!(
        parse_unit(
            "app-glimpse-me.aresa.GlimpsePanel-me.example.My\\x2dClock-left\\x2d0-4242-3.service"
        ),
        Some(("me.example.My-Clock".to_owned(), "left-0".to_owned()))
    );
    assert_eq!(parse_unit("app-glimpse-a-b-c-1-x.service"), None);
    assert_eq!(parse_unit("app-glimpse-a-b-c-x-1.service"), None);
    assert_eq!(parse_unit("app-glimpse-a-b-c--1.service"), None);
    assert_eq!(parse_unit("app-glimpse-a-b-c-1-1.scope"), None);
    assert_eq!(parse_unit("app-glimpse-a-b-c-1.service"), None);
    assert_eq!(parse_unit("app-glimpse-a-b-c-1-2-3.service"), None);
    assert_eq!(parse_unit("other-a-b-c-1-1.service"), None);
}

#[test]
fn runtime_formats_units_with_and_without_details() {
    let since = DateTime::<Utc>::from_timestamp_micros(1_700_000_000_000_000)
        .unwrap()
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let runtime = Runtime {
        status: "running",
        units: vec![
            RuntimeUnit {
                unit: "applet\nunit".to_owned(),
                slot: "0".to_owned(),
                status: "running",
                main_pid: Some(12),
                memory_current: Some(1_572_864),
                restarts: Some(2),
                active_enter_timestamp: Some(1_700_000_000_000_000),
            },
            RuntimeUnit {
                unit: "unavailable.service".to_owned(),
                slot: "1".to_owned(),
                status: "unavailable",
                main_pid: None,
                memory_current: None,
                restarts: None,
                active_enter_timestamp: None,
            },
        ],
    };
    assert_eq!(
        runtime_text(&runtime),
        format!(
            "running\napplet unit  running  pid 12  restarts 2  memory 1.5 MiB  since {since}\nunavailable.service  unavailable"
        )
    );
    assert_eq!(
        runtime_text(&Runtime {
            status: "not running",
            units: vec![]
        }),
        "stopped"
    );
}

#[test]
fn size_boundaries() {
    assert_eq!(size_text(1023), "1023 B");
    assert_eq!(size_text(1024), "1.0 KiB");
    assert_eq!(size_text(1_572_864), "1.5 MiB");
}
