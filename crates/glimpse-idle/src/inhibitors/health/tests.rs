use super::*;

#[test]
fn a_slot_writes_only_its_own_backend() {
    let (health, _changes) = Health::new();
    health.degraded(Backend::Wayland, "no ext-idle-notify-v1");

    let snapshot = health.snapshot();
    assert_eq!(snapshot.wayland.kind, HealthKind::Degraded);
    assert_eq!(snapshot.wayland.message, "no ext-idle-notify-v1");
    assert_eq!(
        snapshot.screen_saver.kind,
        HealthKind::Unsupported,
        "the other three start unknown and stay there"
    );
}

#[test]
fn an_unchanged_write_does_not_bump_the_generation() {
    let (health, mut changes) = Health::new();
    changes.borrow_and_update();

    health.ready(Backend::Login1);
    assert!(changes.has_changed().expect("the sender is live"));
    changes.borrow_and_update();

    health.ready(Backend::Login1);
    assert!(
        !changes.has_changed().expect("the sender is live"),
        "a repeat of the same health would otherwise emit a PropertiesChanged per poll"
    );
}

#[test]
fn a_degraded_message_is_capped_like_any_other_backend_text() {
    let (health, _changes) = Health::new();
    health.degraded(Backend::Portal, "x".repeat(4096));
    assert!(health.snapshot().portal.message.chars().count() <= super::super::WHY_CAP + 2);
}
