use super::*;
use zbus::zvariant::Type;

fn wire(summary: String, body: String, actions: Vec<(String, String)>) -> NotificationWire {
    (
        7,
        "app".to_owned(),
        "App".to_owned(),
        0,
        summary,
        body,
        String::new(),
        String::new(),
        255,
        actions,
        -1.0,
        1_234_567_890,
        true,
        false,
        -1,
    )
}

#[test]
fn wire_sentinels_become_absent_fields_rather_than_defaults() {
    let decoded = decode_notification(wire(
        "Summary".to_owned(),
        String::new(),
        vec![("reply".to_owned(), "Reply".to_owned())],
    ))
    .expect("the wire is readable");

    assert_eq!(decoded.app_pid, None, "pid 0 means absent");
    assert_eq!(decoded.body, None, "an empty body is absent");
    assert_eq!(decoded.icon, None);
    assert_eq!(decoded.image, None);
    assert_eq!(decoded.progress, None, "-1.0 means absent");
    assert_eq!(decoded.urgency, NotificationUrgency::Unknown);
    assert_eq!(decoded.actions[0].key, "reply");
    assert_eq!(decoded.created.timestamp_micros(), 1_234_567_890);
}

#[test]
fn untrusted_text_is_bounded_before_anything_can_render_it() {
    let decoded = decode_notification(wire(
        format!("{}\u{202e}", "s".repeat(SUMMARY + 50)),
        "b".repeat(glimpse_utils::markup::BODY_MAX_CHARS + 50),
        (0..MOST_ACTIONS + 5)
            .map(|n| (format!("key{n}"), "Label".to_owned()))
            .collect(),
    ))
    .expect("the wire is readable");

    assert_eq!(decoded.summary, format!("{}…", "s".repeat(SUMMARY)));
    assert_eq!(
        decoded.body.expect("a body"),
        "b".repeat(glimpse_utils::markup::BODY_MAX_CHARS)
    );
    assert_eq!(decoded.actions.len(), MOST_ACTIONS);
}

#[test]
fn one_unreadable_record_costs_its_row_rather_than_the_whole_surface() {
    let readable = wire("Readable".to_owned(), String::new(), Vec::new());
    let mut unreadable = wire("Unreadable".to_owned(), String::new(), Vec::new());
    unreadable.11 = i64::MAX;

    let view = decode_snapshot((vec![unreadable, readable], (false, 0), true, String::new()))
        .expect("the snapshot is still readable");

    assert_eq!(view.notifications.len(), 1);
    assert_eq!(view.notifications[0].summary, "Readable");
    assert!(view.serving, "the provider is still serving");
}

#[test]
fn wire_signatures_match_the_versioned_contract() {
    assert_eq!(NotificationWire::SIGNATURE, "(ussissssya(ss)dxbbi)");
    assert_eq!(DoNotDisturbWire::SIGNATURE, "(bx)");
    assert_eq!(
        NotificationsSnapshot::SIGNATURE,
        "(a(ussissssya(ss)dxbbi)(bx)bs)"
    );
}

#[test]
fn an_action_key_and_an_application_id_round_trip_unchanged() {
    let mut record = wire(
        "Summary".to_owned(),
        String::new(),
        vec![
            (" reply".to_owned(), "Reply".to_owned()),
            ("open  url".to_owned(), "Open".to_owned()),
            ("a\tb".to_owned(), "Tab".to_owned()),
        ],
    );
    record.1 = "my  app".to_owned();

    let decoded = decode_notification(record).expect("the wire is readable");

    assert_eq!(decoded.app_id, "my  app");
    let keys: Vec<&str> = decoded
        .actions
        .iter()
        .map(|action| action.key.as_str())
        .collect();
    assert_eq!(keys, [" reply", "open  url", "a\tb"]);
}

#[test]
fn an_action_key_that_is_empty_or_over_its_cap_drops_only_that_action() {
    let at_cap = "k".repeat(ACTION_KEY);
    let mut actions = vec![
        (String::new(), "Empty".to_owned()),
        ("k".repeat(ACTION_KEY + 1), "Long".to_owned()),
    ];
    actions.extend((0..MOST_ACTIONS).map(|_| (String::new(), "Dropped".to_owned())));
    actions.push((at_cap.clone(), "Kept".to_owned()));
    actions.push(("reply".to_owned(), "Reply".to_owned()));

    let decoded = decode_notification(wire("Summary".to_owned(), String::new(), actions))
        .expect("the wire is readable");

    assert_eq!(decoded.summary, "Summary");
    let keys: Vec<&str> = decoded
        .actions
        .iter()
        .map(|action| action.key.as_str())
        .collect();
    assert_eq!(keys, [at_cap.as_str(), "reply"]);
}

#[test]
fn an_application_id_that_is_empty_or_over_its_cap_drops_the_notification() {
    let mut empty = wire("Empty".to_owned(), String::new(), Vec::new());
    empty.1 = String::new();
    let mut oversized = wire("Oversized".to_owned(), String::new(), Vec::new());
    oversized.1 = "a".repeat(APP_ID + 1);

    let view = decode_snapshot((
        vec![
            empty,
            oversized,
            wire("Kept".to_owned(), String::new(), Vec::new()),
        ],
        (false, 0),
        true,
        String::new(),
    ))
    .expect("the snapshot is readable");

    let summaries: Vec<&str> = view
        .notifications
        .iter()
        .map(|record| record.summary.as_str())
        .collect();
    assert_eq!(summaries, ["Kept"]);
}

#[test]
fn a_body_keeps_its_markup_and_line_breaks_and_stays_bounded() {
    let kept = decode_notification(wire(
        "s".to_owned(),
        "<b>one</b>\ntwo".to_owned(),
        Vec::new(),
    ))
    .expect("the wire is readable");
    assert_eq!(kept.body.as_deref(), Some("<b>one</b>\ntwo"));

    let long = decode_notification(wire(
        "s".to_owned(),
        format!(
            "<b>{}</b>",
            "b".repeat(glimpse_utils::markup::BODY_MAX_CHARS * 2)
        ),
        Vec::new(),
    ))
    .expect("the wire is readable");
    let body = long.body.expect("a body");
    assert!(body.starts_with("<b>") && body.ends_with("</b>"));
}

fn method_error(name: &str, reason: String) -> zbus::Error {
    let message = zbus::Message::method_call("/", "Probe")
        .unwrap()
        .build(&())
        .unwrap();
    zbus::Error::MethodError(name.try_into().unwrap(), Some(reason), message)
}

#[tokio::test]
async fn a_peer_reason_is_capped_on_every_call_error() {
    let long = "x".repeat(5000);

    let invalid = call(async {
        Err::<(), _>(method_error(
            "me.aresa.Glimpse.Notifications1.Error.InvalidAction",
            long.clone(),
        ))
    })
    .await;
    let Err(NotificationsProviderError::InvalidAction(reason)) = invalid else {
        panic!("an invalid action keeps its variant");
    };
    assert!(reason.chars().count() <= REASON + 1);

    let unavailable = call(async {
        Err::<(), _>(method_error(
            "me.aresa.Glimpse.Notifications1.Error.Unavailable",
            long.clone(),
        ))
    })
    .await;
    let Err(NotificationsProviderError::Unavailable(reason)) = unavailable else {
        panic!("an unavailable provider keeps its variant");
    };
    assert!(reason.chars().count() <= REASON + 1);

    let other = call(async {
        Err::<(), _>(method_error(
            "org.freedesktop.DBus.Error.Failed",
            long.clone(),
        ))
    })
    .await;
    let Err(NotificationsProviderError::Call(reason)) = other else {
        panic!("another failure keeps its variant");
    };
    assert!(reason.chars().count() <= REASON + 1);
}
