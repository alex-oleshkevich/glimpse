use super::*;

#[test]
fn blocks_names_only_the_targets_a_record_holds() {
    assert_eq!(blocks(&InhibitionTargets::manual_hold()), "idle, suspend");
    assert_eq!(blocks(&InhibitionTargets::NONE), "");
}

#[test]
fn inhibitor_rows_show_a_known_expiry_and_leave_other_records_unbounded() {
    let record = IdleInhibitorRecord {
        id: 42,
        who: "test".into(),
        why: String::new(),
        bus_name: String::new(),
        process_name: String::new(),
        source: glimpse_dbus::idle::IdleInhibitorSource::screen_saver(0),
        targets: InhibitionTargets::manual_hold(),
        can_release: true,
        added_at_unix: 0,
    };
    assert_eq!(row(&record, None)[5], "-");
    assert_eq!(
        row(&record, Some(1_800_000_000))[5],
        chrono::DateTime::from_timestamp(1_800_000_000, 0)
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    );
}
