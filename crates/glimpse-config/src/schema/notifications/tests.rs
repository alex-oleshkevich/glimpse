fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, text).expect("writes");
    crate::load_files(std::slice::from_ref(&path))
}

#[test]
fn an_absent_table_keeps_a_hundred() {
    let notifications = load("").expect("an absent table is fine").notifications;

    assert_eq!(notifications.keep, 100);
    assert!(notifications.suppress.is_empty());
    assert!(notifications.enabled);
    assert_eq!(notifications.monitor, None);
    assert_eq!(notifications.edge, super::NotificationEdge::TopRight);
    assert_eq!(notifications.hide_delay, 4);
    assert_eq!(notifications.max_items, 6);
}

#[test]
fn the_bound_reads_from_the_document() {
    assert_eq!(
        load("[notifications]\nkeep = 20\n")
            .expect("keep is a key of this table")
            .notifications
            .keep,
        20
    );
}

#[test]
fn a_setting_this_table_does_not_have_is_refused() {
    let rendered = load("[notifications]\ngroups = 6\n")
        .expect_err("`groups` belongs to the applet, not to this table")
        .to_string();

    assert!(
        rendered.contains("groups"),
        "the error must name `groups`, got {rendered}"
    );
}

#[test]
fn suppression_patterns_are_read_and_default_to_empty() {
    assert!(
        load("")
            .expect("an absent table is fine")
            .notifications
            .suppress
            .is_empty()
    );
    assert_eq!(
        load("[notifications]\nsuppress = [\"(?i)spotify\", \"build succeeded\"]\n")
            .expect("suppression patterns are a key of this table")
            .notifications
            .suppress,
        ["(?i)spotify", "build succeeded"]
    );
}

#[test]
fn popup_settings_read_every_supported_edge() {
    use super::NotificationEdge;

    for (name, expected) in [
        ("top-left", NotificationEdge::TopLeft),
        ("top-center", NotificationEdge::TopCenter),
        ("top-right", NotificationEdge::TopRight),
        ("bottom-left", NotificationEdge::BottomLeft),
        ("bottom-center", NotificationEdge::BottomCenter),
        ("bottom-right", NotificationEdge::BottomRight),
    ] {
        let text = format!(
            "[notifications]\nenabled = false\nmonitor = \"DP-2\"\nedge = \"{name}\"\nhide-delay = 9\nmax-items = 3\n"
        );
        let notifications = load(&text).expect("popup settings load").notifications;
        assert!(!notifications.enabled);
        assert_eq!(notifications.monitor.as_deref(), Some("DP-2"));
        assert_eq!(notifications.edge, expected);
        assert_eq!(notifications.hide_delay, 9);
        assert_eq!(notifications.max_items, 3);
    }
}

#[test]
fn popup_bounds_and_monitor_are_validated() {
    for text in [
        "[notifications]\nhide-delay = 0\n",
        "[notifications]\nhide-delay = 86401\n",
        "[notifications]\nmax-items = 0\n",
        "[notifications]\nmonitor = \"\"\n",
        "[notifications]\nmonitor = \" DP-2\"\n",
        "[notifications]\nedge = \"left\"\n",
    ] {
        assert!(load(text).is_err(), "must reject {text:?}");
    }
}

#[test]
fn storage_and_popup_settings_share_one_table() {
    let notifications = load(
        "[notifications]\nkeep = 20\nsuppress = [\"private\"]\nenabled = false\nedge = \"bottom-right\"\n",
    )
    .expect("the shared table loads")
    .notifications;

    assert_eq!(notifications.keep, 20);
    assert_eq!(notifications.suppress, ["private"]);
    assert!(!notifications.enabled);
    assert_eq!(notifications.edge, super::NotificationEdge::BottomRight);
}

#[test]
fn popup_settings_round_trip_with_their_document_spellings() {
    let notifications = super::Notifications {
        monitor: Some("DP-2".to_owned()),
        edge: super::NotificationEdge::BottomLeft,
        hide_delay: 7,
        max_items: 4,
        ..super::Notifications::default()
    };
    let document = toml::to_string(&notifications).expect("serializes");
    assert!(document.contains("edge = \"bottom-left\""));
    assert!(document.contains("hide-delay = 7"));
    assert!(document.contains("max-items = 4"));
    assert_eq!(
        toml::from_str::<super::Notifications>(&document).expect("deserializes"),
        notifications
    );
}
