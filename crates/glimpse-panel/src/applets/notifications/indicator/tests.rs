use chrono::Utc;
use glimpse_dbus::notifications::{DEFAULT_ACTION, NotificationAction, NotificationUrgency};

use super::*;

fn record(actions: &[&str], unread: bool) -> NotificationRecord {
    NotificationRecord {
        id: 7,
        app_id: "app".to_owned(),
        app_name: "App".to_owned(),
        app_pid: Some(42),
        icon: None,
        image: None,
        summary: "Summary".to_owned(),
        body: None,
        urgency: NotificationUrgency::Normal,
        actions: actions
            .iter()
            .map(|key| NotificationAction {
                key: (*key).to_owned(),
                label: "Action".to_owned(),
            })
            .collect(),
        progress: None,
        created: Utc::now(),
        unread,
        resident: false,
        expire_timeout: -1,
    }
}

#[test]
fn only_a_live_offered_action_becomes_an_invocation() {
    let live = record(&[DEFAULT_ACTION, "reply"], true);
    assert_eq!(invocation(&[live], "7", DEFAULT_ACTION), Some(7));
    assert!(invocation(&[record(&["reply"], true)], "7", DEFAULT_ACTION).is_none());
    assert!(invocation(&[record(&[DEFAULT_ACTION], false)], "7", DEFAULT_ACTION).is_none());
}

#[test]
fn any_unread_card_can_activate_and_default_action_is_optional() {
    for actions in [&[][..], &[DEFAULT_ACTION][..]] {
        let live = record(actions, true);
        assert_eq!(activation(std::slice::from_ref(&live), "7"), Some(live));
    }
    assert!(activation(&[record(&[], false)], "7").is_none());
}

#[test]
fn closing_any_card_removes_it_from_the_popover() {
    assert_eq!(removal(&[record(&[], true)], "7"), Some(7));
    assert_eq!(removal(&[record(&[], false)], "7"), Some(7));
    assert_eq!(removal(&[record(&[], true)], "wrong"), None);
}

#[test]
fn caches_keep_only_keys_present_in_the_latest_list() {
    let mut cache = HashMap::from([("gone".to_owned(), 1), ("kept".to_owned(), 2)]);
    prune_cache(&mut cache, ["kept"].into_iter());

    assert_eq!(cache, HashMap::from([("kept".to_owned(), 2)]));
}

#[test]
fn a_switch_flip_survives_snapshots_until_the_provider_agrees_or_refuses() {
    let pending = Cell::new(Some(true));
    assert!(
        awaited(&pending, false),
        "a snapshot published before the answer must not undo the flip"
    );
    assert!(awaited(&pending, false));
    assert!(awaited(&pending, true));
    assert_eq!(pending.get(), None, "agreement ends the wait");

    pending.set(Some(true));
    pending.set(None);
    assert!(
        !awaited(&pending, false),
        "a refusal clears the wait and the next snapshot shows off"
    );
}
