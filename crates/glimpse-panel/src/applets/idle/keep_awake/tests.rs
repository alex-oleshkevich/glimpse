use super::*;
use glimpse_dbus::idle::{IdleInhibitorSource, InhibitionTargets, Login1Mode};

fn manual(id: u64) -> IdleInhibitorRecord {
    IdleInhibitorRecord {
        id,
        who: "glimpse-idle".to_owned(),
        why: "Manual hold".to_owned(),
        bus_name: String::new(),
        process_name: String::new(),
        source: IdleInhibitorSource::login1(0, 0, Login1Mode::Block),
        targets: InhibitionTargets::manual_hold(),
        can_release: true,
        added_at_unix: id,
    }
}

#[test]
fn a_new_hold_replaces_every_observed_manual_hold_once() {
    let foreign = IdleInhibitorRecord {
        who: "Firefox".to_owned(),
        ..manual(4)
    };
    let snapshot = [manual(1), manual(2), manual(3), manual(3), foreign];
    assert_eq!(hold_ids_to_release(&snapshot, Some(2)), [1, 3]);
    assert_eq!(hold_ids_to_release(&snapshot, None), [1, 2, 3]);
    assert!(hold_ids_to_release(&[], None).is_empty());
}

#[test]
fn already_gone_is_a_successful_release() {
    assert!(release_result(Err(IdleProviderError::UnknownObject)).is_ok());
    assert!(release_result(Err(IdleProviderError::TimedOut)).is_err());
}

#[test]
fn newest_manual_hold_controls_the_deadline() {
    let timed = manual(1);
    let expiries = BTreeMap::from([(1, 901), (3, 1_803)]);
    assert_eq!(
        deadline(std::slice::from_ref(&timed), &expiries).unwrap().1,
        900
    );
    assert_eq!(deadline(&[timed.clone(), manual(2)], &expiries), None);
    assert_eq!(deadline(&[timed, manual(3)], &expiries).unwrap().1, 1_800);
    assert_eq!(deadline(&[manual(1)], &BTreeMap::new()), None);
}
