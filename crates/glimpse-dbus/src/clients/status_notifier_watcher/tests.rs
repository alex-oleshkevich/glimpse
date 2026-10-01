use super::{Registry, canonical_key, split_key};

#[test]
fn a_bus_name_argument_falls_back_to_the_default_path() {
    assert_eq!(
        canonical_key(":1.42", "org.kde.StatusNotifierItem-8123-1"),
        ":1.42/StatusNotifierItem"
    );
}

#[test]
fn a_path_argument_is_taken_as_the_path_and_the_sender_as_the_owner() {
    assert_eq!(
        canonical_key(":1.42", "/org/ayatana/NotificationItem/fake"),
        ":1.42/org/ayatana/NotificationItem/fake"
    );
}

#[test]
fn a_key_splits_at_the_first_slash_because_a_bus_name_has_none() {
    assert_eq!(
        split_key(":1.42/org/ayatana/NotificationItem/fake"),
        Some((":1.42", "/org/ayatana/NotificationItem/fake"))
    );
    assert_eq!(split_key("nothing-here"), None);
}

#[test]
fn registering_twice_adds_one_entry() {
    let mut registry = Registry::default();
    assert!(registry.register(":1.1", "/StatusNotifierItem").is_some());
    assert!(
        registry
            .register(":1.1", "org.kde.StatusNotifierItem-1-1")
            .is_none()
    );
    assert_eq!(registry.items(), [":1.1/StatusNotifierItem"]);
}

#[test]
fn unregister_removes_exactly_one_and_reports_nothing_for_a_stranger() {
    let mut registry = Registry::default();
    registry.register(":1.1", "/StatusNotifierItem");
    registry.register(":1.2", "/StatusNotifierItem");
    assert_eq!(
        registry
            .unregister(":1.1", "/StatusNotifierItem")
            .as_deref(),
        Some(":1.1/StatusNotifierItem")
    );
    assert_eq!(registry.items(), [":1.2/StatusNotifierItem"]);
    assert!(registry.unregister(":1.9", "/StatusNotifierItem").is_none());
}

#[test]
fn one_owner_may_hold_several_items_and_loses_all_of_them_together() {
    let mut registry = Registry::default();
    registry.register(":1.1", "/StatusNotifierItem");
    registry.register(":1.1", "/org/ayatana/NotificationItem/second");
    registry.register(":1.2", "/StatusNotifierItem");

    assert_eq!(
        registry.evict_owner(":1.1"),
        [
            ":1.1/StatusNotifierItem",
            ":1.1/org/ayatana/NotificationItem/second"
        ]
    );
    assert_eq!(registry.items(), [":1.2/StatusNotifierItem"]);
}

#[test]
fn a_resync_takes_the_foreign_order_and_reports_what_left() {
    let mut registry = Registry::default();
    registry.register(":1.1", "/StatusNotifierItem");
    registry.register(":1.2", "/StatusNotifierItem");

    assert_eq!(
        registry.resync(&[
            ":1.2/StatusNotifierItem".to_owned(),
            ":1.3/StatusNotifierItem".to_owned(),
        ]),
        [":1.1/StatusNotifierItem"],
        "what the watcher no longer lists is what we drop"
    );
    assert_eq!(
        registry.items(),
        [":1.2/StatusNotifierItem", ":1.3/StatusNotifierItem"],
        "and the order is the watcher's, not ours"
    );
}

#[test]
fn a_host_is_registered_once_and_leaves_with_its_owner() {
    let mut registry = Registry::default();
    assert!(registry.register_host(":1.5"));
    assert!(!registry.register_host(":1.5"));
    assert!(registry.host_registered());
    registry.evict_owner(":1.5");
    assert!(!registry.host_registered());
}
