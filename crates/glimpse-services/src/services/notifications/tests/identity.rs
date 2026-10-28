use super::*;

#[test]
fn a_body_is_sanitised_on_the_way_in_and_an_empty_one_is_absent_rather_than_blank() {
    let marked = record(
        1,
        Incoming {
            body: "<b>Marta</b> sent <script>alert(1)</script>a file".to_owned(),
            ..incoming("Telegram", "Marta")
        },
    );
    let body = marked.body.expect("a body that has text");
    assert!(
        body.contains("<b>Marta</b>"),
        "the markup a sender may send survives: {body}"
    );
    assert!(
        !body.contains("script"),
        "the markup it may not does not: {body}"
    );

    let blank = record(
        2,
        Incoming {
            body: "   ".to_owned(),
            ..incoming("Telegram", "Marta")
        },
    );
    assert_eq!(blank.body, None);
}

#[test]
fn a_process_desktop_entry_stabilizes_short_lived_bus_senders() {
    let first = application_id(
        None,
        Some("walz".to_owned()),
        "walz",
        Some(":1.195415".to_owned()),
        1,
    );
    let second = application_id(
        None,
        Some("walz".to_owned()),
        "walz",
        Some(":1.197442".to_owned()),
        2,
    );

    assert_eq!(first, "walz");
    assert_eq!(second, first);
    assert_eq!(
        application_id(
            Some("org.example.Chat".to_owned()),
            Some("walz".to_owned()),
            "walz",
            Some(":1.197442".to_owned()),
            3,
        ),
        "org.example.Chat"
    );
}

#[test]
fn a_sender_that_exits_between_notifications_still_groups_by_the_name_it_calls_itself() {
    let shot = |sender: &str, id| application_id(None, None, "niri", Some(sender.to_owned()), id);

    assert_eq!(
        shot(":1.333903", 1),
        shot(":1.334too", 2),
        "a screenshot script opens a new connection per notification, and a unique bus name \
             would put every one of them in a group of its own"
    );
    assert_eq!(shot(":1.333903", 1), "name:niri");
    assert_ne!(
        application_id(
            None,
            None,
            "org.telegram.desktop",
            Some(":1.9".to_owned()),
            8
        ),
        application_id(
            Some("org.telegram.desktop".to_owned()),
            None,
            "whatever",
            Some(":1.10".to_owned()),
            9,
        ),
        "a sender with no identity of its own cannot join a group by claiming its name"
    );
    assert_eq!(
        application_id(None, None, "", Some(":1.42".to_owned()), 7),
        ":1.42",
        "a sender that names itself nothing still has to land somewhere"
    );
}

#[tokio::test]
async fn a_desktop_entry_resolves_its_display_name_before_storage() {
    let name = "services::notifications::tests::identity::a_desktop_entry_resolves_its_display_name_before_storage";
    if std::env::var_os("GLIMPSE_DESKTOP_TEST").is_none() {
        let scratch = tempfile::tempdir().expect("scratch directory");
        let applications = scratch.path().join("applications");
        std::fs::create_dir(&applications).expect("desktop directory");
        std::fs::write(
            applications.join("org.example.NotificationFixture.desktop"),
            "[Desktop Entry]\nType=Application\nName=Notification Fixture\nExec=true\n",
        )
        .expect("desktop entry");
        assert!(
            std::process::Command::new(std::env::current_exe().expect("test binary"))
                .args(["--exact", name])
                .env("GLIMPSE_DESKTOP_TEST", "1")
                .env("XDG_DATA_HOME", scratch.path())
                .env("LC_ALL", "C")
                .status()
                .expect("isolated test runs")
                .success()
        );
        return;
    }
    let (app_id, app_name) = resolve_sender(
        Some("org.example.NotificationFixture".to_owned()),
        None,
        String::new(),
        None,
        1,
    )
    .await;
    let mut held = Store::new(10);
    held.post(
        1,
        Incoming {
            app_id,
            app_name,
            ..incoming("", "hello")
        },
    );
    assert_eq!(held.records()[0].app_name, "Notification Fixture");
}

#[test]
fn an_empty_app_name_uses_the_app_id_when_no_desktop_name_resolved() {
    assert_eq!(
        application_name("", "com.mitchellh.ghostty"),
        "com.mitchellh.ghostty"
    );
    assert_eq!(
        application_name("", ":1.42"),
        "",
        "a synthetic identity has no desktop name"
    );
}

#[test]
fn a_declared_app_name_remains_authoritative() {
    assert_eq!(
        application_name("Ghostty Developer Build", "com.mitchellh.ghostty"),
        "Ghostty Developer Build"
    );
}

#[test]
fn an_app_id_that_is_blank_is_never_stored() {
    assert_eq!(
        application_id(
            Some(" ".to_owned()),
            None,
            "Telegram",
            Some(":1.42".to_owned()),
            5
        ),
        "name:Telegram",
        "a whitespace desktop-entry hint is no hint"
    );
    for blank in ["", " ", "\t\n"] {
        let stored = record(
            5,
            Incoming {
                app_id: blank.to_owned(),
                ..incoming("Telegram", "Marta")
            },
        );
        assert_eq!(stored.app_id, "notification-5", "blank {blank:?}");
    }
    let named = record(
        6,
        Incoming {
            app_id: "org.telegram.desktop".to_owned(),
            ..incoming("Telegram", "Marta")
        },
    );
    assert_eq!(named.app_id, "org.telegram.desktop");
}
