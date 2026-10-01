use super::*;

fn entry(what: &str, who: &str, why: &str, mode: &str, pid: u32) -> Login1InhibitorEntry {
    (what.into(), who.into(), why.into(), mode.into(), 1000, pid)
}
#[test]
fn diff_detects_additions_excluding_own_pid() {
    let previous = HashMap::new();
    let current = vec![
        entry("idle", "firefox", "video", "block", 1234),
        entry("idle:sleep", "glimpse-idle", "Manual hold", "block", 9999),
    ];

    let diff = compute_diff(&previous, &current, 9999);

    assert_eq!(diff.added.len(), 1);
    assert_eq!(diff.added[0].1, "firefox");
    assert!(diff.removed.is_empty());
}
#[test]
fn diff_deduplicates_same_key_entries_within_one_snapshot() {
    let previous = HashMap::new();
    let current = vec![
        entry("idle", "backup-tool", "snapshotting", "block", 4242),
        entry("sleep", "backup-tool", "snapshotting", "block-weak", 4242),
    ];

    let diff = compute_diff(&previous, &current, 9999);

    assert_eq!(
        diff.added.len(),
        1,
        "the duplicate key must collapse to one addition"
    );
    assert_eq!(
        diff.added[0].0, "idle",
        "the first occurrence must be the one kept"
    );
}
#[test]
fn diff_filters_delay_mode_but_keeps_block_and_block_weak() {
    let previous = HashMap::new();
    let current = vec![
        entry("idle", "firefox", "video", "block", 1234),
        entry("sleep", "NetworkManager", "cleanup", "delay", 555),
        entry("shutdown", "ModemManager", "cleanup", "delay", 666),
        entry("idle", "apt", "upgrade", "block-weak", 777),
    ];

    let diff = compute_diff(&previous, &current, 9999);

    let who: Vec<&str> = diff.added.iter().map(|entry| entry.1.as_str()).collect();
    assert!(who.contains(&"firefox"));
    assert!(who.contains(&"apt"));
    assert!(!who.contains(&"NetworkManager"));
    assert!(!who.contains(&"ModemManager"));
}
#[test]
fn diff_drops_entries_with_an_unrecognized_mode() {
    let previous = HashMap::new();
    let current = vec![entry("idle", "mystery", "why", "bogus-mode", 1234)];

    let diff = compute_diff(&previous, &current, 9999);

    assert!(diff.added.is_empty());
}
#[test]
fn diff_detects_removals() {
    let mut previous = HashMap::new();
    previous.insert(
        (1234u32, "firefox".to_owned(), "video".to_owned()),
        ObservedInhibitor {
            id: 1,
            process_name: "firefox".into(),
        },
    );
    previous.insert(
        (5678u32, "apt".to_owned(), "upgrade".to_owned()),
        ObservedInhibitor {
            id: 2,
            process_name: "apt".into(),
        },
    );
    let current = vec![entry("idle", "firefox", "video", "block", 1234)];

    let diff = compute_diff(&previous, &current, 9999);

    assert!(diff.added.is_empty());
    assert_eq!(
        diff.removed,
        vec![(5678u32, "apt".to_owned(), "upgrade".to_owned())]
    );
}

#[test]
fn parse_login1_mode_maps_the_three_known_strings_and_nothing_else() {
    assert_eq!(parse_login1_mode("block"), Some(Login1Mode::Block));
    assert_eq!(parse_login1_mode("block-weak"), Some(Login1Mode::BlockWeak));
    assert_eq!(parse_login1_mode("delay"), Some(Login1Mode::Delay));
    assert_eq!(parse_login1_mode("anything-else"), None);
}
#[test]
fn entry_to_record_carries_pid_uid_and_targets_and_never_allows_release() {
    let e = entry("sleep:shutdown", "apt", "upgrade", "block", 1234);
    let record = entry_to_record(&e, 42, "apt".to_owned());

    assert_eq!(record.id, 42);
    assert_eq!(record.who, "apt");
    assert_eq!(record.why, "upgrade");
    assert_eq!(record.process_name, "apt");
    assert!(record.targets.suspend);
    assert!(record.targets.shutdown);
    assert!(!record.targets.idle);
    assert!(!record.can_release);
    assert_eq!(record.bus_name, "");
    assert_eq!(record.source.kind, glimpse_dbus::idle::SourceKind::Login1);
    assert_eq!(record.source.pid, 1234);
    assert_eq!(record.source.uid, 1000);
    assert_eq!(record.source.mode, Login1Mode::Block);
}
#[test]
fn compute_renames_only_reports_ids_whose_process_name_actually_changed() {
    let mut observed = HashMap::new();
    let unchanged_key = (1234u32, "firefox".to_owned(), "video".to_owned());
    let changed_key = (5678u32, "apt".to_owned(), "upgrade".to_owned());
    observed.insert(
        unchanged_key.clone(),
        ObservedInhibitor {
            id: 1,
            process_name: "firefox".into(),
        },
    );
    observed.insert(
        changed_key.clone(),
        ObservedInhibitor {
            id: 2,
            process_name: "apt".into(),
        },
    );

    let mut fresh_names = HashMap::new();
    fresh_names.insert(unchanged_key, "firefox".to_owned());
    fresh_names.insert(changed_key, "dpkg".to_owned());

    let renames = compute_renames(&observed, &fresh_names);

    assert_eq!(renames.len(), 1);
    assert_eq!(renames[0].id, 2);
    assert_eq!(renames[0].process_name, "dpkg");
}

#[test]
fn compute_renames_reports_nothing_when_no_fresh_name_is_available() {
    let mut observed = HashMap::new();
    observed.insert(
        (1234u32, "firefox".to_owned(), "video".to_owned()),
        ObservedInhibitor {
            id: 1,
            process_name: "firefox".into(),
        },
    );

    let renames = compute_renames(&observed, &HashMap::new());

    assert!(renames.is_empty());
}
#[tokio::test]
async fn read_process_name_returns_none_for_a_pid_with_no_readable_comm() {
    assert_eq!(read_process_name(u32::MAX).await, None);
}
