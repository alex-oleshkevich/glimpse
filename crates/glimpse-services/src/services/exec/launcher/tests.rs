use super::*;

#[test]
fn unit_naming_escapes_panel_and_applet() {
    assert_eq!(
        unit_name("me.aresa.GlimpsePanel-test", "me.example.My-Applet", 3, 7),
        "app-glimpse-me.aresa.GlimpsePanel\\x2dtest-me.example.My\\x2dApplet-3-7.service"
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
    assert!(get("Restart").is_some_and(|v| v.contains("on-failure")));
    assert!(get("RestartUSec").is_some_and(|v| v.contains("1000000")));
    assert!(get("RestartMaxDelayUSec").is_some_and(|v| v.contains("60000000")));
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
