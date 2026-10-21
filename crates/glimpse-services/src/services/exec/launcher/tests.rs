use super::*;

#[test]
fn unit_naming_escapes_panel_and_applet() {
    assert_eq!(
        unit_name("me.aresa.GlimpsePanel-test", "me.example.My-Applet", 3, 7),
        format!(
            "app-glimpse-me.aresa.GlimpsePanel\\x2dtest-me.example.My\\x2dApplet-3-{}-7.service",
            std::process::id()
        )
    );
    assert_eq!(
        sweep_prefix("me.aresa.GlimpsePanel-test"),
        "app-glimpse-me.aresa.GlimpsePanel\\x2dtest-"
    );
    assert!(unit_name("panel-a", "x", 1, 1).starts_with(&sweep_prefix("panel-a")));
    assert!(!unit_name("panel-a", "x", 1, 1).starts_with(&sweep_prefix("panel-b")));
}

#[test]
fn env_filters_systemd_vars_and_config_wins() {
    let panel = vec![
        ("DISPLAY".to_owned(), ":0".to_owned()),
        ("INVOCATION_ID".to_owned(), "abc".to_owned()),
        ("JOURNAL_STREAM".to_owned(), "1:2".to_owned()),
        ("NOTIFY_SOCKET".to_owned(), "/run/x".to_owned()),
        ("LISTEN_FDS".to_owned(), "1".to_owned()),
        ("LISTEN_PID".to_owned(), "2".to_owned()),
        ("MANAGERPID".to_owned(), "3".to_owned()),
        ("SYSTEMD_EXEC_PID".to_owned(), "4".to_owned()),
        ("WATCHDOG_USEC".to_owned(), "5".to_owned()),
        ("BASH_FUNC_foo%%".to_owned(), "() { :; }".to_owned()),
        ("FOO".to_owned(), "panel".to_owned()),
        ("XDG_RUNTIME_DIR".to_owned(), "/run/user/1000".to_owned()),
    ];
    let config = BTreeMap::from([("FOO".to_owned(), "1".to_owned())]);
    let env = build_env(panel, Path::new("/run/glimpse/a.sock"), &config);
    let get = |name: &str| {
        env.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(get("DISPLAY"), Some(":0"));
    assert_eq!(get("XDG_RUNTIME_DIR"), Some("/run/user/1000"));
    assert_eq!(get("FOO"), Some("1"));
    assert_eq!(get("GLIMPSE_APPLET_SOCKET"), Some("/run/glimpse/a.sock"));
    for denied in [
        "INVOCATION_ID",
        "JOURNAL_STREAM",
        "NOTIFY_SOCKET",
        "LISTEN_FDS",
        "LISTEN_PID",
        "MANAGERPID",
        "SYSTEMD_EXEC_PID",
        "WATCHDOG_USEC",
        "BASH_FUNC_foo%%",
    ] {
        assert_eq!(get(denied), None, "{denied}");
    }
    assert_eq!(
        env.iter().filter(|(key, _)| key == "FOO").count(),
        1,
        "config env replaces, never duplicates"
    );
}

#[test]
fn program_resolution_uses_the_panel_path() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let program = dir.path().join("my-applet");
    std::fs::write(&program, "#!/bin/sh\n").expect("program");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let plain = dir.path().join("plain");
    std::fs::write(&plain, "x").expect("plain file");
    let path = dir.path().display().to_string();
    assert_eq!(
        resolve_program("my-applet", Some(&path)).as_deref(),
        Some(program.to_string_lossy().as_ref())
    );
    assert_eq!(resolve_program("plain", Some(&path)), None);
    assert_eq!(resolve_program("missing", Some(&path)), None);
    assert_eq!(
        resolve_program("/opt/bin/my-applet", Some(&path)).as_deref(),
        Some("/opt/bin/my-applet")
    );
    assert_eq!(resolve_program("my-applet", None), None);
}

#[test]
fn spec_building_resolves_argv0_or_fails() {
    let panel = vec![("PATH".to_owned(), "/usr/bin:/bin".to_owned())];
    let spec = build_spec(
        "app-glimpse-x.service".to_owned(),
        "Test".to_owned(),
        vec!["sh".to_owned(), "-c".to_owned(), "true".to_owned()],
        None,
        panel.clone(),
        Path::new("/run/a.sock"),
        &BTreeMap::new(),
        true,
        None,
    )
    .expect("spec");
    assert!(spec.argv[0].ends_with("/sh"), "{}", spec.argv[0]);
    assert_eq!(spec.argv[1..], ["-c", "true"]);
    let failed = build_spec(
        "app-glimpse-x.service".to_owned(),
        "Test".to_owned(),
        vec!["definitely-not-a-program-xyz".to_owned()],
        None,
        panel,
        Path::new("/run/a.sock"),
        &BTreeMap::new(),
        true,
        None,
    );
    assert_eq!(
        failed.err().as_deref(),
        Some("definitely-not-a-program-xyz not found")
    );
}

#[test]
fn spec_building_uses_the_configured_path_without_a_panel_path() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("program dir");
    let program = dir.path().join("configured-applet");
    std::fs::write(&program, "#!/bin/sh\n").expect("program");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let path = dir.path().display().to_string();
    let spec = build_spec(
        "app-glimpse-x.service".to_owned(),
        "Test".to_owned(),
        vec!["configured-applet".to_owned()],
        None,
        Vec::new(),
        Path::new("/run/glimpse/a.sock"),
        &BTreeMap::from([("PATH".to_owned(), path.clone())]),
        true,
        None,
    )
    .expect("configured PATH resolves the executable");
    assert_eq!(spec.argv, vec![program.display().to_string()]);
    assert_eq!(
        spec.env.iter().find(|(key, _)| key == "PATH"),
        Some(&("PATH".to_owned(), path))
    );
}

#[test]
fn unit_properties_carry_the_restart_curve() {
    let spec = UnitSpec {
        name: "app-glimpse-x.service".to_owned(),
        description: "Test".to_owned(),
        argv: vec!["/usr/bin/true".to_owned()],
        cwd: Some(PathBuf::from("/tmp")),
        env: vec![("FOO".to_owned(), "1".to_owned())],
        restart: true,
        stderr: None,
    };
    let props = unit_properties(&spec).expect("properties");
    let get = |name: &str| {
        props
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| format!("{value:?}"))
    };
    assert!(get("Restart").is_some_and(|v| v.contains("always")));
    assert!(get("RestartUSec").is_some_and(|v| v.contains("1000000")));
    assert_eq!(
        props
            .iter()
            .find(|(key, _)| *key == "RestartSteps")
            .map(|(_, value)| value),
        Some(&zbus::zvariant::Value::from(6u32))
    );
    assert!(get("RestartMaxDelayUSec").is_some_and(|v| v.contains("60000000")));
    assert_eq!(
        props
            .iter()
            .find(|(key, _)| *key == "StartLimitIntervalUSec")
            .map(|(_, value)| value),
        Some(&zbus::zvariant::Value::from(0u64))
    );
    assert!(get("MemoryMax").is_some_and(|v| v.contains("268435456")));
    assert!(get("WorkingDirectory").is_some_and(|v| v.contains("/tmp")));
    assert!(get("Environment").is_some_and(|v| v.contains("FOO=1")));
    let dev = UnitSpec {
        restart: false,
        ..spec
    };
    let props = unit_properties(&dev).expect("dev properties");
    assert!(
        props
            .iter()
            .find(|(key, _)| *key == "Restart")
            .is_some_and(|(_, value)| format!("{value:?}").contains("\"no\""))
    );
}

#[test]
fn unit_properties_stop_restarting_when_the_panel_is_gone() {
    let spec = UnitSpec {
        name: "app-glimpse-x.service".to_owned(),
        description: "Test".to_owned(),
        argv: vec!["/usr/bin/true".to_owned()],
        cwd: None,
        env: Vec::new(),
        restart: true,
        stderr: None,
    };
    let props = unit_properties(&spec).expect("properties");
    let value = props
        .iter()
        .find(|(key, _)| *key == "RestartPreventExitStatus")
        .map(|(_, value)| value)
        .expect("RestartPreventExitStatus");
    assert_eq!(
        value,
        &zbus::zvariant::Value::from((vec![69i32], Vec::<i32>::new()))
    );
    assert_eq!(value.value_signature().to_string(), "(aiai)");
    assert_eq!(
        <(Vec<i32>, Vec<i32>)>::try_from(value).expect("(aiai)"),
        (vec![69], Vec::new())
    );
}

#[test]
fn panel_pid_parses_only_exec_unit_names() {
    assert_eq!(
        panel_pid("app-glimpse-me.aresa.GlimpsePanel-me.example.Test-1-765616-2.service"),
        Some(765616)
    );
    assert_eq!(
        panel_pid(&unit_name("panel-a", "applet-b", 3, 7)),
        Some(std::process::id())
    );
    for name in [
        "app-glimpse-firefox-765616-1.service",
        "app-glimpse-panel-applet-1-765616.service",
        "app-glimpse-panel-applet-1-765616-1-extra.service",
        "app-glimpse-panel-applet-x-765616-1.service",
        "app-glimpse-panel-applet-1-x-1.service",
        "app-glimpse-panel-applet-1-765616-x.service",
        "app-glimpse-panel-applet-1--1.service",
        "app-glimpse--applet-1-765616-1.service",
        "app-glimpse-panel--1-765616-1.service",
        "app-glimpse-panel-applet-1-765616-1",
        "glimpse-panel-applet-1-765616-1.service",
    ] {
        assert_eq!(panel_pid(name), None, "{name}");
    }
}

#[test]
fn orphan_units_belong_to_dead_foreign_panels() {
    let name = "app-glimpse-me.aresa.Other-me.example.Test-1-765616-1.service";
    let own = std::process::id();
    assert!(is_orphan_unit(name, own, |_| false));
    assert!(is_orphan_unit(name, own, |pid| pid != 765616));
    assert!(!is_orphan_unit(name, own, |_| true));
    assert!(!is_orphan_unit(name, 765616, |_| false));
    assert!(!is_orphan_unit(
        "app-glimpse-firefox-765616-1.service",
        own,
        |_| false
    ));
    assert!(!is_orphan_unit("not-a-unit", own, |_| false));
}

async fn watched_gone(mut watch: BoxStream<'static, UnitState>) -> bool {
    use futures_util::StreamExt;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(state) = watch.next().await {
            if state == UnitState::Gone {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false)
}

#[tokio::test]
async fn process_sweep_stops_orphans_and_spares_live_panels() {
    use futures_util::StreamExt;
    let launcher = ProcessLauncher::new();
    let panel = "me.aresa.GlimpsePanel";
    let mine = unit_name(panel, "me.example.Test", 1, 1);
    let orphan = "app-glimpse-me.aresa.DeadPanel-me.example.Test-1-4194999-1.service".to_owned();
    let live = unit_name("me.aresa.OtherPanel", "me.example.Test", 1, 1);
    for name in [&mine, &orphan, &live] {
        launcher
            .start(UnitSpec {
                name: name.clone(),
                description: "sleeper".to_owned(),
                argv: vec!["/bin/sleep".to_owned(), "30".to_owned()],
                cwd: None,
                env: Vec::new(),
                restart: false,
                stderr: None,
            })
            .await
            .expect("start sleeper");
    }
    let mine_watch = launcher.watch(&mine);
    let orphan_watch = launcher.watch(&orphan);
    let mut live_watch = launcher.watch(&live);
    launcher.sweep(&sweep_prefix(panel)).await;
    assert!(watched_gone(mine_watch).await, "own unit swept");
    assert!(watched_gone(orphan_watch).await, "orphan swept");
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(200), live_watch.next())
            .await
            .is_err(),
        "a live panel's unit untouched"
    );
    launcher.stop(&live).await;
}

#[tokio::test]
async fn missing_connection_needs_the_user_manager() {
    let launcher = SystemdLauncher::new(None);
    let spec = UnitSpec {
        name: "app-glimpse-x.service".to_owned(),
        description: "Test".to_owned(),
        argv: vec!["/usr/bin/true".to_owned()],
        cwd: None,
        env: Vec::new(),
        restart: true,
        stderr: None,
    };
    assert_eq!(
        launcher.start(spec).await.err().as_deref(),
        Some("needs the systemd user manager")
    );
}

#[tokio::test]
#[ignore = "needs the systemd user manager"]
async fn sweep_matches_escaped_panel_ids_and_preserves_other_panels() {
    let connection = zbus::Connection::session().await.expect("session bus");
    let manager = Systemd1ManagerProxy::new(&connection)
        .await
        .expect("manager");
    let launcher = SystemdLauncher::new(Some(connection.clone()));
    let panel = format!("me.aresa.Sweep-Test{}", std::process::id());
    let name = unit_name(&panel, "me.example.Test", 1, 1);
    let other = unit_name(&format!("{panel}-other"), "me.example.Test", 1, 1);
    for name in [&name, &other] {
        launcher
            .start(UnitSpec {
                name: name.clone(),
                description: "Sweep test".to_owned(),
                argv: vec!["/usr/bin/sleep".to_owned(), "30".to_owned()],
                cwd: None,
                env: Vec::new(),
                restart: false,
                stderr: None,
            })
            .await
            .expect("start test unit");
    }
    let other_unit = Systemd1UnitProxy::builder(&connection)
        .path(manager.get_unit(&other).await.expect("other unit"))
        .expect("unit path")
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .expect("unit proxy");
    let started = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !other_unit
            .active_state()
            .await
            .is_ok_and(|state| state == "active")
        {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    launcher.sweep(&sweep_prefix(&panel)).await;
    let swept = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while manager.get_unit(&name).await.is_ok() {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    let preserved = other_unit
        .active_state()
        .await
        .is_ok_and(|state| state == "active");
    launcher.stop(&name).await;
    launcher.stop(&other).await;
    assert!(started.is_ok(), "other panel's unit must start");
    assert!(swept.is_ok(), "escaped panel prefix must match its unit");
    assert!(preserved, "another panel's unit must remain running");
}

struct FakeManager;

#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl FakeManager {
    fn subscribe(&self) {}

    fn get_unit(&self, _name: &str) -> zbus::zvariant::OwnedObjectPath {
        zbus::zvariant::OwnedObjectPath::try_from("/org/freedesktop/systemd1/unit/applet").unwrap()
    }

    #[zbus(signal)]
    async fn unit_removed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        id: String,
        unit: zbus::zvariant::OwnedObjectPath,
    ) -> zbus::Result<()>;
}

struct FakeUnit;

#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl FakeUnit {
    #[zbus(property)]
    fn sub_state(&self) -> String {
        "running".to_owned()
    }

    #[zbus(property)]
    fn active_state(&self) -> String {
        "active".to_owned()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unpolled_watch_never_stalls_the_connection() {
    use futures_util::StreamExt;
    use glimpse_dbus::testing::PrivateBus;
    use std::time::Duration;

    let bus = PrivateBus::start();
    let systemd = bus.connection().await;
    systemd
        .object_server()
        .at("/org/freedesktop/systemd1", FakeManager)
        .await
        .unwrap();
    systemd
        .object_server()
        .at("/org/freedesktop/systemd1/unit/applet", FakeUnit)
        .await
        .unwrap();
    glimpse_dbus::own_name(&systemd, "org.freedesktop.systemd1")
        .await
        .unwrap();

    let panel = bus.connection().await;
    let launcher = SystemdLauncher::new(Some(panel.clone()));
    let mut watch = launcher.watch("applet.service");
    let _ = tokio::time::timeout(Duration::from_millis(500), watch.next()).await;

    let emitter =
        zbus::object_server::SignalEmitter::new(&systemd, "/org/freedesktop/systemd1").unwrap();
    for n in 0..200 {
        FakeManager::unit_removed(
            &emitter,
            format!("other-{n}.service"),
            zbus::zvariant::OwnedObjectPath::try_from("/org/freedesktop/systemd1/unit/other")
                .unwrap(),
        )
        .await
        .unwrap();
    }

    let answered = tokio::time::timeout(
        Duration::from_secs(3),
        zbus::fdo::DBusProxy::new(&panel)
            .await
            .unwrap()
            .list_names(),
    )
    .await;
    assert!(answered.is_ok(), "the connection stopped being read");

    FakeManager::unit_removed(
        &emitter,
        "applet.service".to_owned(),
        zbus::zvariant::OwnedObjectPath::try_from("/org/freedesktop/systemd1/unit/applet").unwrap(),
    )
    .await
    .unwrap();
    let state = tokio::time::timeout(Duration::from_secs(3), watch.next())
        .await
        .expect("the watch never reported the removal");
    assert!(matches!(state, Some(UnitState::Gone)), "{state:?}");
}
