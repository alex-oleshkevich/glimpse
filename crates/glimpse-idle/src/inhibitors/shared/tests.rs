use super::*;
use glimpse_dbus::idle::{IdleInhibitorRecord, IdleInhibitorSource, InhibitionTargets, Login1Mode};

fn idle_record(id: u64) -> IdleInhibitorRecord {
    IdleInhibitorRecord {
        id,
        who: "who".into(),
        why: "why".into(),
        bus_name: String::new(),
        process_name: String::new(),
        source: IdleInhibitorSource::login1(0, 0, Login1Mode::Block),
        targets: InhibitionTargets::idle_only(),
        can_release: true,
        added_at_unix: 0,
    }
}

#[tokio::test]
async fn mutate_republishes_any_idle_target_only_when_it_changes() {
    let (shared, mut receiver, _generation) = SharedRegistry::new();
    assert!(!*receiver.borrow_and_update());

    let id = shared.mutate(|registry| registry.mint_id()).await;
    shared
        .mutate(|registry| registry.insert(idle_record(id), None))
        .await;
    assert!(receiver.has_changed().unwrap());
    assert!(*receiver.borrow_and_update());

    shared.mutate(|registry| registry.mint_id()).await;
    assert!(
        !receiver.has_changed().unwrap(),
        "a mutation that leaves any_idle_target unchanged must not wake the subscriber"
    );

    shared.mutate(|registry| registry.release_record(id)).await;
    assert!(receiver.has_changed().unwrap());
    assert!(!*receiver.borrow_and_update());
}

#[tokio::test]
async fn read_sees_what_mutate_wrote() {
    let (shared, _receiver, _generation) = SharedRegistry::new();
    let id = shared.mutate(|registry| registry.mint_id()).await;
    shared
        .mutate(|registry| registry.insert(idle_record(id), None))
        .await;

    let snapshot = shared.read(|registry| registry.snapshot()).await;
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].id, id);
}

#[tokio::test]
async fn generation_tracks_registry_version_and_skips_state_preserving_mutations() {
    let (shared, _idle_target, mut generation) = SharedRegistry::new();
    assert_eq!(*generation.borrow_and_update(), 0);

    shared.mutate(|registry| registry.mint_id()).await;
    assert!(
        !generation.has_changed().unwrap(),
        "minting alone must not bump generation"
    );

    let id = shared.mutate(|registry| registry.mint_id()).await;
    shared
        .mutate(|registry| registry.insert(idle_record(id), None))
        .await;
    assert!(generation.has_changed().unwrap());
    assert_eq!(*generation.borrow_and_update(), 1);

    shared
        .mutate(|registry| registry.set_process_name(id, "firefox".to_owned()))
        .await;
    assert!(generation.has_changed().unwrap());
    assert_eq!(*generation.borrow_and_update(), 2);

    shared.mutate(|registry| registry.release_record(id)).await;
    assert!(generation.has_changed().unwrap());
    assert_eq!(*generation.borrow_and_update(), 3);

    shared.mutate(|registry| registry.release_record(id)).await;
    assert!(!generation.has_changed().unwrap());
}
