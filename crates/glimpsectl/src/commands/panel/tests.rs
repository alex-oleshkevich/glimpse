use super::*;
use glimpse_dbus::{own_name, testing::PrivateBus};

fn value(value: Value<'_>) -> OwnedValue {
    OwnedValue::try_from(value).unwrap()
}

fn row(connector: &str, position: Position, name: &str, own_view: bool) -> AppletRow {
    AppletRow {
        action: "applet.1".into(),
        connector: connector.into(),
        position,
        zone: "left".into(),
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
        decode("applet.1", &state("left", 2)).unwrap(),
    ];
    sort(&mut rows);
    assert_eq!(rows[0].zone, "left");
    assert_eq!(rows[1].zone, "center");
    assert_eq!(rows[0].indicators[0].icon.as_deref(), Some("weather-clear"));
    assert_eq!(rows[0].indicators[0].label.as_deref(), Some("Sunny"));
    assert_eq!(rows[0].indicators[0].attention, Some(true));
    assert_eq!(rows[0].indicators[0].badge, None);
    let sparse = value(Value::from(HashMap::from([
        ("connector".to_owned(), value(Value::from("DP-1"))),
        ("position".to_owned(), value(Value::from("top"))),
        ("zone".to_owned(), value(Value::from("left"))),
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

#[test]
fn live_targets_require_one_match_across_outputs_and_edges() {
    let rows = [
        row("DP-1", Position::Top, "clock", false),
        row("DP-2", Position::Top, "clock", false),
        row("DP-2", Position::Bottom, "clock", false),
    ];
    let mut target = AppletTarget {
        name: "clock".into(),
        output: None,
        edge: None,
    };
    assert!(
        resolve_live(&rows, &target)
            .unwrap_err()
            .0
            .contains("ambiguous")
    );
    target.output = Some("DP-2".into());
    assert!(resolve_live(&rows, &target).is_err());
    target.edge = Some(Position::Bottom);
    assert_eq!(
        resolve_live(&rows, &target).unwrap().position,
        Position::Bottom
    );
    target.name = "missing".into();
    assert!(
        resolve_live(&rows, &target)
            .unwrap_err()
            .0
            .contains("no matching applet")
    );
    let duplicated = [
        row("DP-2", Position::Bottom, "clock", false),
        row("DP-2", Position::Bottom, "clock", false),
    ];
    target.name = "clock".into();
    assert!(resolve_live(&duplicated, &target).is_err());
}

#[tokio::test]
async fn panel_discovery_includes_custom_ids_only_with_owned_hosts() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    own_name(&server, "me.test.CustomPanel").await.unwrap();
    own_name(&server, "me.test.CustomPanel.AppletHost")
        .await
        .unwrap();
    own_name(&server, "me.test.Orphan.AppletHost")
        .await
        .unwrap();
    own_name(&server, "me.aresa.Glimpse.Weather").await.unwrap();
    let client = bus.connection().await;
    assert_eq!(
        panel_names(&client).await.unwrap(),
        vec!["me.test.CustomPanel"]
    );
    server.release_name("me.test.CustomPanel").await.unwrap();
    assert!(panel_names(&client).await.unwrap().is_empty());
}

#[tokio::test]
async fn ambiguous_live_mutations_do_not_activate_any_action() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Actions {
        descriptions: Descriptions,
        hits: Arc<AtomicUsize>,
    }
    #[zbus::interface(name = "org.gtk.Actions")]
    impl Actions {
        fn describe_all(&self) -> Descriptions {
            self.descriptions.clone()
        }
        fn activate(
            &self,
            _action: &str,
            _parameters: Vec<OwnedValue>,
            _platform_data: HashMap<String, OwnedValue>,
        ) {
            self.hits.fetch_add(1, Ordering::Relaxed);
        }
    }
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let app_id = "me.test.Panel";
    own_name(&server, app_id).await.unwrap();
    let mut descriptions = Descriptions::new();
    for (serial, output) in [(1, "DP-1"), (2, "DP-2")] {
        let state = value(Value::from(HashMap::from([
            ("connector".to_owned(), value(Value::from(output))),
            ("position".to_owned(), value(Value::from("top"))),
            ("zone".to_owned(), value(Value::from("left"))),
            ("index".to_owned(), value(Value::from(0_u32))),
            ("name".to_owned(), value(Value::from("clock"))),
            ("alive".to_owned(), value(Value::from(true))),
            ("own-view".to_owned(), value(Value::from(false))),
            ("popover".to_owned(), value(Value::from(false))),
        ])));
        descriptions.insert(
            format!("applet.{serial}"),
            (true, "(sv)".try_into().unwrap(), vec![state]),
        );
    }
    let hits = Arc::new(AtomicUsize::new(0));
    server
        .object_server()
        .at(
            object_path(app_id),
            Actions {
                descriptions,
                hits: hits.clone(),
            },
        )
        .await
        .unwrap();
    let client = bus.connection().await;
    let target = || AppletTarget {
        name: "clock".into(),
        output: None,
        edge: None,
    };
    let error = applet_live(
        &client,
        app_id,
        AppletCommand::Click {
            target: target(),
            button: 1,
        },
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(crate::errors::exit(&error), crate::errors::Exit::Rejected);
    assert_eq!(hits.load(Ordering::Relaxed), 0);
    let mut target = target();
    target.output = Some("DP-2".into());
    applet_live(
        &client,
        app_id,
        AppletCommand::Click { target, button: 1 },
        true,
    )
    .await
    .unwrap();
    assert_eq!(hits.load(Ordering::Relaxed), 1);
}

fn metadata(version: &str, bars: Option<Vec<(u32, &str, &str)>>) -> Descriptions {
    let mut descriptions = Descriptions::from([(
        "version".into(),
        (
            false,
            "".try_into().unwrap(),
            vec![value(Value::from(version))],
        ),
    )]);
    if let Some(bars) = bars {
        descriptions.insert(
            "bars".into(),
            (
                false,
                "".try_into().unwrap(),
                vec![value(Value::from(bars))],
            ),
        );
    }
    descriptions
}

struct MetadataActions(Descriptions);

#[zbus::interface(name = "org.gtk.Actions")]
impl MetadataActions {
    fn describe_all(&self) -> Descriptions {
        self.0.clone()
    }
}

#[tokio::test]
async fn panel_host_metadata_accepts_separate_connections_of_the_same_process() {
    let bus = PrivateBus::start();
    let gtk = bus.connection().await;
    let host = bus.connection().await;
    let client = bus.connection().await;
    let app_id = "me.test.Panel";
    own_name(&gtk, app_id).await.unwrap();
    gtk.object_server()
        .at(object_path(app_id), MetadataActions(metadata("test", None)))
        .await
        .unwrap();
    let host_name = glimpse_dbus::applet_host_name(app_id);
    own_name(&host, host_name.as_str()).await.unwrap();
    let owner = gtk.unique_name().unwrap().to_owned();
    assert_ne!(host.unique_name(), gtk.unique_name());
    let dbus = zbus::fdo::DBusProxy::new(&client).await.unwrap();
    let row = panel_row(&client, &dbus, app_id.into(), owner.clone(), None)
        .await
        .unwrap();
    assert_eq!(row.pid, Some(std::process::id()));
    assert!(row.applet_host);
    host.release_name(host_name.as_str()).await.unwrap();
    let row = panel_row(&client, &dbus, app_id.into(), owner, None)
        .await
        .unwrap();
    assert!(!row.applet_host);
}

#[tokio::test]
async fn panel_host_metadata_rejects_another_process() {
    let app_id = "me.test.Panel";
    let host_name = glimpse_dbus::applet_host_name(app_id);
    if let Ok(address) = std::env::var("GLIMPSE_PANEL_HOST_METADATA_BUS") {
        use std::io::Write;
        let connection = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        own_name(&connection, host_name.as_str()).await.unwrap();
        println!("HOST_READY");
        std::io::stdout().flush().unwrap();
        std::future::pending::<()>().await;
        return;
    }
    use std::{process::Stdio, time::Duration};
    use tokio::io::{AsyncBufReadExt, BufReader};
    let bus = PrivateBus::start();
    let gtk = bus.connection().await;
    own_name(&gtk, app_id).await.unwrap();
    gtk.object_server()
        .at(object_path(app_id), MetadataActions(metadata("test", None)))
        .await
        .unwrap();
    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "commands::panel::tests::panel_host_metadata_rejects_another_process",
            "--nocapture",
        ])
        .env("GLIMPSE_PANEL_HOST_METADATA_BUS", bus.address())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if line.contains("HOST_READY") {
                return;
            }
        }
        panic!("host owner exited before taking its name");
    })
    .await
    .unwrap();
    let client = bus.connection().await;
    let dbus = zbus::fdo::DBusProxy::new(&client).await.unwrap();
    let host_pid = dbus
        .get_connection_unix_process_id(host_name.as_str().try_into().unwrap())
        .await
        .unwrap();
    assert_eq!(Some(host_pid), child.id());
    assert_ne!(host_pid, std::process::id());
    let row = panel_row(
        &client,
        &dbus,
        app_id.into(),
        gtk.unique_name().unwrap().to_owned(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(row.pid, Some(std::process::id()));
    assert!(!row.applet_host);
    child.kill().await.unwrap();
    child.wait().await.unwrap();
}

#[tokio::test]
async fn panel_metadata_reads_empty_bars_and_is_pinned_to_one_owner() {
    let bus = PrivateBus::start();
    let old = bus.connection().await;
    let replacement = bus.connection().await;
    let client = bus.connection().await;
    let app_id = "me.test.Panel";
    own_name(&old, app_id).await.unwrap();
    let owner = old.unique_name().unwrap().to_owned();
    old.object_server()
        .at(
            object_path(app_id),
            MetadataActions(metadata(
                "old",
                Some(vec![(1, "DP-1", "top"), (2, "DP-1", "bottom")]),
            )),
        )
        .await
        .unwrap();
    replacement
        .object_server()
        .at(
            object_path(app_id),
            MetadataActions(metadata("replacement", None)),
        )
        .await
        .unwrap();
    old.release_name(app_id).await.unwrap();
    own_name(&replacement, app_id).await.unwrap();
    let dbus = zbus::fdo::DBusProxy::new(&client).await.unwrap();
    let row = panel_row(&client, &dbus, app_id.into(), owner, None)
        .await
        .unwrap();
    assert_eq!(row.version.as_deref(), Some("old"));
    assert_eq!(
        row.bars.unwrap(),
        [
            BarRow {
                index: 1,
                connector: "DP-1".into(),
                position: Position::Top
            },
            BarRow {
                index: 2,
                connector: "DP-1".into(),
                position: Position::Bottom
            }
        ]
    );
    assert_eq!(row.applet_count, Some(0));
    let row = panel_row(
        &client,
        &dbus,
        app_id.into(),
        replacement.unique_name().unwrap().to_owned(),
        None,
    )
    .await
    .unwrap();
    assert!(row.bars.is_none());
    let missing: zbus::names::OwnedUniqueName = ":1.999999".try_into().unwrap();
    let row = panel_row(&client, &dbus, app_id.into(), missing, None)
        .await
        .unwrap();
    assert!(row.pid.is_none());
    assert!(row.version.is_none());
    assert!(row.bars.is_none());
}
