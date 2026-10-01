use chrono::TimeZone as _;
use glimpse_dbus::notifications::{DEFAULT_ACTION, NotificationAction, NotificationUrgency};

use super::*;

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 10, hour, minute, 0).unwrap()
}

fn record(id: u32, app: &str, summary: &str) -> NotificationRecord {
    NotificationRecord {
        id,
        app_id: app.to_owned(),
        app_name: app.to_owned(),
        app_pid: None,
        summary: summary.to_owned(),
        body: None,
        icon: None,
        image: None,
        urgency: NotificationUrgency::Normal,
        actions: Vec::new(),
        progress: None,
        created: at(12, 0),
        unread: true,
        resident: false,
        expire_timeout: -1,
    }
}

#[test]
fn groups_keep_newest_first_and_gather_by_app_id() {
    let records = [
        record(3, "slack", "third"),
        record(2, "mail", "second"),
        record(1, "slack", "first"),
    ];
    let groups = groups(&records, at(12, 1));
    assert_eq!(
        groups
            .iter()
            .map(|group| group.key.as_str())
            .collect::<Vec<_>>(),
        ["slack", "mail"]
    );
    assert_eq!(
        groups[0]
            .notifications
            .iter()
            .map(|note| note.summary.as_str())
            .collect::<Vec<_>>(),
        ["third", "first"]
    );
}

#[test]
fn a_card_click_is_identified_by_the_store_id() {
    assert_eq!(id_of("42"), Some(42));
    assert_eq!(id_of("nope"), None);
}

#[test]
fn the_named_default_action_is_the_card_not_a_button() {
    let mut note = record(1, "app", "hi");
    note.actions = vec![
        NotificationAction {
            key: "default".to_owned(),
            label: "Open".to_owned(),
        },
        NotificationAction {
            key: "reply".to_owned(),
            label: "Reply".to_owned(),
        },
    ];
    let [group] = groups(&[note], at(12, 1)).try_into().unwrap();
    assert_eq!(
        group.notifications[0]
            .actions
            .iter()
            .map(|action| action.key.as_str())
            .collect::<Vec<_>>(),
        ["reply"]
    );
    assert!(group.notifications[0].activatable);
}

#[test]
fn every_unread_card_is_activatable_even_without_a_default_action() {
    let absent = groups(&[record(1, "app", "new")], at(12, 1));
    assert!(absent[0].notifications[0].activatable);

    let mut read = record(2, "app", "old");
    read.unread = false;
    read.actions = vec![NotificationAction {
        key: DEFAULT_ACTION.to_owned(),
        label: "Open".to_owned(),
    }];
    let read = groups(&[read], at(12, 1));
    assert!(!read[0].notifications[0].activatable);
    assert!(read[0].notifications[0].actions.is_empty());
}

#[test]
fn age_is_compact_and_never_negative() {
    let created = at(12, 0);
    assert_eq!(when(at(12, 0), created), "now");
    assert_eq!(when(at(12, 2), created), "2m");
    assert_eq!(when(at(14, 0), created), "2h");
    assert_eq!(
        when(
            at(12, 0).checked_add_signed(TimeDelta::days(3)).unwrap(),
            created
        ),
        "3d"
    );
    assert_eq!(when(at(11, 0), created), "now");
}

#[test]
fn every_indicator_style_has_one_unambiguous_unread_treatment() {
    let records = [record(1, "a", "one"), record(2, "b", "two")];
    let icon = chip(&records, false, NotificationIndicatorStyle::IconOnly);
    assert_eq!(icon.badge, None);
    assert!(!icon.attention);

    let dot = chip(&records, false, NotificationIndicatorStyle::IconDot);
    assert_eq!(dot.badge, None);
    assert!(dot.attention);

    let counter = chip(&records, false, NotificationIndicatorStyle::IconCounter);
    assert_eq!(counter.badge.as_deref(), Some("2"));
    assert!(
        !counter.attention,
        "the count is the treatment; a badge suppresses the dot, so attention would render nothing"
    );

    for style in [
        NotificationIndicatorStyle::IconOnly,
        NotificationIndicatorStyle::IconDot,
        NotificationIndicatorStyle::IconCounter,
    ] {
        let quiet = chip(&records, true, style);
        assert_eq!(quiet.icon, MUTED);
        assert_eq!(quiet.badge, None);
        assert!(!quiet.attention);
        assert_eq!(quiet.severity, Some(Severity::Info));
    }
}

#[test]
fn a_critical_unread_notification_colors_only_the_dot() {
    let mut note = record(1, "a", "low battery");
    note.urgency = NotificationUrgency::Critical;
    for (style, severity, attention) in [
        (NotificationIndicatorStyle::IconOnly, None, false),
        (
            NotificationIndicatorStyle::IconDot,
            Some(Severity::Error),
            true,
        ),
        (NotificationIndicatorStyle::IconCounter, None, false),
    ] {
        let shown = chip(std::slice::from_ref(&note), false, style);
        assert_eq!(shown.severity, severity);
        assert_eq!(shown.attention, attention);
    }
}

#[test]
fn an_empty_list_is_a_bell_with_no_badge() {
    let shown = chip(&[], false, NotificationIndicatorStyle::IconCounter);
    assert_eq!(shown.icon, BELL);
    assert_eq!(shown.badge, None);
    assert!(!shown.attention);
    assert_eq!(shown.severity, None);
}

#[test]
fn tooltip_count_token_is_the_unread_total() {
    let records = [record(1, "a", "one")];
    assert_eq!(
        tooltip(&records, false, Some("{count} waiting")),
        "1 waiting"
    );
}
