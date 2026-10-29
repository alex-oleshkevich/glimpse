use super::*;
use chrono::TimeZone;

fn record(
    id: u32,
    app_id: &str,
    app_name: &str,
    created: DateTime<Utc>,
    urgency: NotificationUrgency,
    icon: Option<&str>,
) -> NotificationRecord {
    NotificationRecord {
        id,
        app_id: app_id.to_owned(),
        app_name: app_name.to_owned(),
        app_pid: None,
        summary: "Summary".to_owned(),
        body: None,
        icon: icon.map(str::to_owned),
        image: None,
        urgency,
        actions: Vec::new(),
        progress: None,
        created,
        unread: true,
        resident: false,
        expire_timeout: -1,
    }
}

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + seconds, 0).unwrap()
}

#[test]
fn a_record_older_than_the_lock_is_dropped() {
    let records = vec![record(
        1,
        "app",
        "App",
        at(-10),
        NotificationUrgency::Normal,
        None,
    )];
    let groups = groups_of(&records, at(0));
    assert!(
        groups.is_empty(),
        "the since filter excludes anything before the lock began"
    );
}

#[test]
fn a_record_at_the_lock_moment_is_kept() {
    let records = vec![record(
        1,
        "app",
        "App",
        at(0),
        NotificationUrgency::Normal,
        None,
    )];
    let groups = groups_of(&records, at(0));
    assert_eq!(groups.len(), 1, "at or after the lock moment is inclusive");
}

#[test]
fn records_group_by_app_id_never_by_app_name() {
    let records = vec![
        record(
            1,
            "org.app",
            "App One",
            at(1),
            NotificationUrgency::Normal,
            None,
        ),
        record(
            2,
            "org.app",
            "App Two",
            at(2),
            NotificationUrgency::Normal,
            None,
        ),
    ];
    let groups = groups_of(&records, at(0));
    assert_eq!(
        groups.len(),
        1,
        "the same app_id is one group even with a different app_name"
    );
    assert_eq!(groups[0].count, 2);
    assert_eq!(
        groups[0].app, "App Two",
        "the most recent record's app_name labels the group"
    );
}

#[test]
fn a_different_app_id_never_merges_even_with_the_same_app_name() {
    let records = vec![
        record(
            1,
            "org.app.one",
            "Same Name",
            at(1),
            NotificationUrgency::Normal,
            None,
        ),
        record(
            2,
            "org.app.two",
            "Same Name",
            at(2),
            NotificationUrgency::Normal,
            None,
        ),
    ];
    let groups = groups_of(&records, at(0));
    assert_eq!(groups.len(), 2, "app_id, not app_name, is the grouping key");
}

#[test]
fn a_critical_group_sorts_before_a_more_recent_normal_one() {
    let records = vec![
        record(
            1,
            "normal",
            "Normal",
            at(5),
            NotificationUrgency::Normal,
            None,
        ),
        record(
            2,
            "critical",
            "Critical",
            at(1),
            NotificationUrgency::Critical,
            None,
        ),
    ];
    let groups = groups_of(&records, at(0));
    assert_eq!(
        groups[0].key, "critical",
        "critical goes first even though it is older"
    );
    assert_eq!(groups[1].key, "normal");
}

#[test]
fn groups_otherwise_sort_by_most_recent() {
    let records = vec![
        record(1, "old", "Old", at(1), NotificationUrgency::Normal, None),
        record(2, "new", "New", at(5), NotificationUrgency::Normal, None),
    ];
    let groups = groups_of(&records, at(0));
    assert_eq!(groups[0].key, "new");
    assert_eq!(groups[1].key, "old");
}

#[test]
fn no_records_since_the_lock_produce_no_groups() {
    assert!(groups_of(&[], at(0)).is_empty());
}

#[test]
fn a_missing_icon_on_the_most_recent_record_falls_back_to_an_older_one_in_the_group() {
    let records = vec![
        record(
            1,
            "app",
            "App",
            at(1),
            NotificationUrgency::Normal,
            Some("older-icon"),
        ),
        record(2, "app", "App", at(2), NotificationUrgency::Normal, None),
    ];
    let groups = groups_of(&records, at(0));
    assert!(
        groups[0].icon.is_some(),
        "an older record in the same group still supplies an icon"
    );
}

#[test]
fn a_group_with_no_icon_anywhere_falls_back_to_the_generic_icon() {
    let records = vec![record(
        1,
        "app",
        "App",
        at(1),
        NotificationUrgency::Normal,
        None,
    )];
    let groups = groups_of(&records, at(0));
    assert!(
        groups[0].icon.is_some(),
        "nothing in the group has an icon, so the generic one fills in"
    );
}
