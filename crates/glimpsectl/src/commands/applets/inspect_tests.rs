use super::*;

#[test]
fn config_formats_empty_and_granted_placement() {
    assert_eq!(config_text(&[]), "not placed");
    let place = Placement {
        instance: "clock\nmain".to_owned(),
        zone: "left".to_owned(),
        panel: 1,
        options: Map::from_iter([("label".to_owned(), Value::from("Work"))]),
        sandbox: AppletSandbox {
            read: vec!["/tmp/read\npath".to_owned()],
            write: vec!["/tmp/write".to_owned()],
            network: true,
            unconfined: true,
        },
    };
    assert_eq!(
        config_text(&[place]),
        "panel 1 · left · clock main\n  options: label = \"Work\"\n  sandbox: read /tmp/read path; write /tmp/write; network yes; unconfined"
    );
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
                status: "running",
                pids: Some(vec![12, 34]),
                memory_current: Some(1_572_864),
                active_enter_timestamp: Some(1_700_000_000_000_000),
            },
            RuntimeUnit {
                unit: "unavailable.service".to_owned(),
                status: "unavailable",
                pids: None,
                memory_current: None,
                active_enter_timestamp: None,
            },
        ],
    };
    assert_eq!(
        runtime_text(&runtime),
        format!(
            "running\napplet unit  running  pid 12, 34  memory 1.5 MiB  since {since}\nunavailable.service  unavailable"
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
