use std::io::Read;
use std::os::fd::OwnedFd as StdOwnedFd;

use glimpse_dbus::idle::{IdleInhibitorSource, InhibitionTargets, Login1Mode};

use super::*;

fn test_record(id: u64, bus_name: &str, idle: bool) -> IdleInhibitorRecord {
    IdleInhibitorRecord {
        id,
        who: "who".into(),
        why: "why".into(),
        bus_name: bus_name.into(),
        process_name: String::new(),
        source: IdleInhibitorSource::login1(0, 0, Login1Mode::Block),
        targets: if idle {
            InhibitionTargets::idle_only()
        } else {
            InhibitionTargets::NONE
        },
        can_release: true,
        added_at_unix: 0,
    }
}

#[test]
fn cookie_round_trips_through_insert_and_release() {
    let mut r = Registry::default();
    let id = r.mint_id();
    let cookie = r.mint_cookie();
    let record = IdleInhibitorRecord {
        source: IdleInhibitorSource::screen_saver(cookie),
        ..test_record(id, ":1.1", false)
    };
    r.insert(record, None);
    assert_eq!(r.lookup_by_cookie(cookie), Some(id));
    r.release_record(id);
    assert_eq!(r.lookup_by_cookie(cookie), None);
}

#[test]
fn release_record_of_unknown_id_returns_none() {
    let mut r = Registry::default();
    assert!(r.release_record(999).is_none());
}

#[test]
fn release_record_closes_the_held_logind_fd() {
    let mut r = Registry::default();
    let id = r.mint_id();
    let (mut reader, writer) = std::io::pipe().expect("create a test pipe");
    let write_fd: StdOwnedFd = writer.into();
    r.insert(test_record(id, ":1.1", false), Some(write_fd.into()));

    assert_eq!(r.release_record(id), Some(id));

    let mut buf = [0u8; 1];
    let read = reader
        .read(&mut buf)
        .expect("read from the pipe after release");
    assert_eq!(
        read, 0,
        "EOF only appears once every fd referencing the write end is closed"
    );
}

#[test]
fn attach_fd_holds_the_fd_until_release_and_drops_it_for_a_gone_record() {
    let mut r = Registry::default();
    let id = r.mint_id();
    r.insert(test_record(id, ":1.1", false), None);

    let (mut reader, writer) = std::io::pipe().expect("create a test pipe");
    let write_fd: StdOwnedFd = writer.into();
    assert!(r.attach_fd(id, write_fd.into()));
    r.release_record(id);
    let mut buf = [0u8; 1];
    assert_eq!(reader.read(&mut buf).expect("read after release"), 0);

    let (mut reader, writer) = std::io::pipe().expect("create a test pipe");
    let write_fd: StdOwnedFd = writer.into();
    assert!(!r.attach_fd(id, write_fd.into()));
    assert_eq!(
        reader.read(&mut buf).expect("read after a refused attach"),
        0
    );
}

#[test]
fn bus_name_to_ids_tracks_multiple_records_and_release_by_bus_name_clears_them() {
    let mut r = Registry::default();
    let id1 = r.mint_id();
    let cookie1 = r.mint_cookie();
    r.insert(
        IdleInhibitorRecord {
            source: IdleInhibitorSource::screen_saver(cookie1),
            ..test_record(id1, ":1.5", false)
        },
        None,
    );
    let id2 = r.mint_id();
    let cookie2 = r.mint_cookie();
    r.insert(
        IdleInhibitorRecord {
            source: IdleInhibitorSource::screen_saver(cookie2),
            ..test_record(id2, ":1.5", false)
        },
        None,
    );
    assert_eq!(r.bus_name_to_ids[":1.5"], [id1, id2]);
    assert_eq!(r.records.len(), 2);

    let released = r.release_by_bus_name(":1.5");
    assert_eq!(released.len(), 2);
    assert!(released.contains(&id1));
    assert!(released.contains(&id2));
    assert_eq!(r.lookup_by_cookie(cookie1), None);
    assert_eq!(r.lookup_by_cookie(cookie2), None);
    assert!(
        !r.bus_name_to_ids.contains_key(":1.5"),
        "the bus-name key itself must be removed, not left behind as an empty Vec"
    );
    assert_eq!(r.records.len(), 0);
    assert!(r.release_by_bus_name(":1.5").is_empty());
}

#[test]
fn a_portal_app_id_counts_against_the_per_source_cap_like_a_bus_name() {
    let mut r = Registry::default();
    for _ in 0..Registry::MAX_INHIBITORS_PER_BUS {
        let id = r.mint_id();
        let record = IdleInhibitorRecord {
            source: IdleInhibitorSource::portal(format!("/req/{id}"), "org.hostile.App"),
            bus_name: String::new(),
            ..test_record(id, "", false)
        };
        r.insert(record, None);
    }

    assert!(
        r.check_capacity(Some("org.hostile.App")).is_err(),
        "a portal record carries no bus name, so counting only bus names lets one sandboxed \
             app take every global slot"
    );
    assert!(r.check_capacity(Some("org.polite.App")).is_ok());
}

#[test]
fn portal_records_with_an_empty_app_id_share_one_per_source_cap() {
    let mut r = Registry::default();
    for n in 0..Registry::MAX_INHIBITORS_PER_BUS {
        let id = r.mint_id();
        let record = IdleInhibitorRecord {
            source: IdleInhibitorSource::portal(format!("/req/{id}"), ""),
            ..test_record(id, &format!(":1.{n}"), false)
        };
        r.insert(record, None);
    }

    assert!(r.check_capacity(Some("")).is_err());
    assert!(r.check_capacity(Some("org.polite.App")).is_ok());
}

#[test]
fn an_empty_owner_does_not_count_non_portal_or_named_records() {
    let mut r = Registry::default();
    for n in 0..Registry::MAX_INHIBITORS_PER_BUS {
        let id = r.mint_id();
        r.insert(test_record(id, &format!(":1.{n}"), false), None);
        let id = r.mint_id();
        let record = IdleInhibitorRecord {
            source: IdleInhibitorSource::portal(format!("/req/{id}"), "org.named.App"),
            ..test_record(id, "", false)
        };
        r.insert(record, None);
    }

    assert!(r.check_capacity(Some("")).is_ok());
}

#[test]
fn check_capacity_enforces_per_bus_limit_independently_per_bus_name() {
    let mut r = Registry::default();
    for _ in 0..Registry::MAX_INHIBITORS_PER_BUS {
        let id = r.mint_id();
        let cookie = r.mint_cookie();
        r.insert(
            IdleInhibitorRecord {
                source: IdleInhibitorSource::screen_saver(cookie),
                ..test_record(id, ":1.9", false)
            },
            None,
        );
    }
    assert!(r.check_capacity(Some(":1.9")).is_err());
    assert!(r.check_capacity(Some(":1.10")).is_ok());
    assert!(r.check_capacity(None).is_ok());
}

#[test]
fn check_capacity_enforces_the_global_cap_even_when_a_bus_has_room() {
    let mut r = Registry::default();
    for i in 0..Registry::MAX_INHIBITORS_TOTAL {
        let id = r.mint_id();
        let bus_name = format!(":1.{i}");
        r.insert(test_record(id, &bus_name, false), None);
    }
    assert!(r.check_capacity(Some(":1.0")).is_err());
    assert!(r.check_capacity(Some("brand-new-bus-name")).is_err());
}

#[test]
fn any_idle_target_reflects_the_live_record_set() {
    let mut r = Registry::default();
    assert!(!r.any_idle_target());
    let id = r.mint_id();
    r.insert(test_record(id, ":1.1", true), None);
    assert!(r.any_idle_target());
    r.release_record(id);
    assert!(!r.any_idle_target());
}

#[test]
fn any_idle_target_stays_false_for_a_non_idle_targeting_record() {
    let mut r = Registry::default();
    let id = r.mint_id();
    r.insert(test_record(id, ":1.1", false), None);
    assert!(!r.any_idle_target());
}

#[test]
fn set_process_name_updates_the_record_and_is_a_no_op_for_an_unknown_id() {
    let mut r = Registry::default();
    let id = r.mint_id();
    r.insert(test_record(id, ":1.1", false), None);

    r.set_process_name(id, "firefox".to_owned());
    assert_eq!(r.snapshot()[0].process_name, "firefox");

    r.set_process_name(999, "nobody".to_owned());
    assert_eq!(r.snapshot()[0].process_name, "firefox");
}

#[test]
fn version_bumps_only_on_state_changing_operations() {
    let mut r = Registry::default();
    let before = r.version();

    let id = r.mint_id();
    let _cookie = r.mint_cookie();
    assert_eq!(r.version(), before, "minting alone must not bump version");

    r.insert(test_record(id, ":1.1", false), None);
    assert_eq!(r.version(), before + 1);

    let _ = r.check_capacity(Some(":1.1"));
    assert_eq!(
        r.version(),
        before + 1,
        "checks alone must not bump version"
    );

    r.set_process_name(id, "firefox".to_owned());
    assert_eq!(r.version(), before + 2);

    r.set_process_name(999, "nobody".to_owned());
    assert_eq!(
        r.version(),
        before + 2,
        "set_process_name for an unknown id must not bump version"
    );

    r.release_record(id);
    assert_eq!(r.version(), before + 3);

    assert!(r.release_record(id).is_none());
    assert_eq!(
        r.version(),
        before + 3,
        "releasing an already-gone id must not bump version"
    );
}

#[test]
fn mint_id_and_mint_cookie_never_repeat_and_cookie_zero_is_never_issued() {
    let mut r = Registry::default();
    let mut ids = std::collections::HashSet::new();
    let mut cookies = std::collections::HashSet::new();
    for _ in 0..1000 {
        let id = r.mint_id();
        assert_ne!(
            id, 0,
            "an id of 0 reads as a sentinel, not a real inhibitor"
        );
        assert!(ids.insert(id), "duplicate id minted");
        let cookie = r.mint_cookie();
        assert_ne!(cookie, 0);
        assert!(cookies.insert(cookie), "duplicate cookie minted");
    }
}

#[test]
fn snapshot_is_ordered_by_id() {
    let mut r = Registry::default();
    for n in 0..32 {
        let id = r.mint_id();
        r.insert(test_record(id, &format!(":1.{n}"), false), None);
    }

    let ids: Vec<u64> = r.snapshot().iter().map(|record| record.id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
}
