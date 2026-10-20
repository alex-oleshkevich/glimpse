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
fn the_display_name_does_not_choose_the_group_identity() {
    let declared = record(
        2,
        Incoming {
            app_id: "org.telegram.desktop".to_owned(),
            ..incoming("Telegram", "Marta")
        },
    );
    assert_eq!(
        declared.app_id, "org.telegram.desktop",
        "a sender with a desktop identity is keyed on it, never on the name it chose"
    );
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

#[test]
fn an_empty_app_name_falls_back_to_the_desktop_identity() {
    let mut held = Store::new(10);
    assert!(held.post(
        48,
        Incoming {
            app_name: String::new(),
            app_id: "com.mitchellh.ghostty".to_owned(),
            ..incoming("", "Ghostty")
        }
    ));

    let records = held.records();
    assert!(
        !records[0].app_name.is_empty(),
        "Ghostty supplies a desktop entry but leaves the freedesktop app-name argument empty"
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
