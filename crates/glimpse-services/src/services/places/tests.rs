use glimpse_dbus::Buses;
use tokio_util::sync::CancellationToken;

use super::*;

async fn places(
    config: Config,
) -> (
    Places,
    Ctx<Places>,
    tokio::sync::watch::Receiver<PlacesState>,
    tokio::sync::watch::Receiver<crate::ServiceState>,
) {
    let cancel = CancellationToken::new();
    let (events, _inbox) = tokio::sync::mpsc::channel(8);
    let (state, state_rx) = tokio::sync::watch::channel(PlacesState::default());
    let (health, health_rx) = tokio::sync::watch::channel(crate::ServiceState::Starting);
    let ctx = Ctx::<Places>::new(
        events,
        &cancel,
        state,
        health,
        Buses::unavailable("no bus in tests"),
    );
    let service = Places::start(&ctx, config, ()).await;
    (service, ctx, state_rx, health_rx)
}

fn enabled() -> Config {
    Config {
        trash: true,
        network: true,
    }
}

#[tokio::test]
async fn trash_and_network_are_neither_read_nor_watched_when_turned_off() {
    let (service, ..) = places(Config {
        trash: false,
        network: false,
    })
    .await;

    let declared = service.subscriptions();
    let keys: Vec<&Watch> = declared.iter().map(Sub::key).collect();

    assert!(keys.contains(&&Watch::Xdg));
    assert!(keys.contains(&&Watch::Bookmarks));
    assert!(
        !keys.contains(&&Watch::Trash),
        "trash must not be declared when turned off"
    );
    assert!(
        !keys.contains(&&Watch::Network),
        "network must not be declared when turned off"
    );
}

#[tokio::test]
async fn trash_and_network_are_declared_when_turned_on() {
    let (service, ..) = places(enabled()).await;

    let declared = service.subscriptions();
    let keys: Vec<&Watch> = declared.iter().map(Sub::key).collect();

    assert!(keys.contains(&&Watch::Trash));
    assert!(keys.contains(&&Watch::Network));
}

#[tokio::test]
async fn a_failed_source_publishes_nothing_and_degrades_health() {
    let (mut service, ctx, state, health) = places(enabled()).await;

    service
        .handle(
            &ctx,
            Input::Event(Event::Network(Ok(vec![Place {
                id: "share".to_owned(),
                name: "share".to_owned(),
                path: PathBuf::from("/run/user/1000/gvfs/share"),
                kind: Kind::Network,
            }]))),
        )
        .await;
    assert_eq!(state.borrow().network.len(), 1);

    service
        .handle(
            &ctx,
            Input::Event(Event::Network(Err(
                "its directory cannot be read: permission denied".to_owned(),
            ))),
        )
        .await;

    assert_eq!(
        state.borrow().network.len(),
        1,
        "a failed read must not overwrite the last known state"
    );
    assert!(matches!(
        &*health.borrow(),
        crate::ServiceState::Degraded { .. }
    ));
}

#[tokio::test]
async fn health_recovers_once_every_failing_source_succeeds_again() {
    let (mut service, ctx, _state, health) = places(enabled()).await;

    service
        .handle(
            &ctx,
            Input::Event(Event::Trash(Err(
                "its directory cannot be read: other".to_owned()
            ))),
        )
        .await;
    assert!(matches!(
        &*health.borrow(),
        crate::ServiceState::Degraded { .. }
    ));

    service
        .handle(&ctx, Input::Event(Event::Trash(Ok(0))))
        .await;
    assert!(matches!(&*health.borrow(), crate::ServiceState::Running));
}

#[test]
fn emptying_the_trash_clears_files_and_their_sidecars_but_keeps_the_folders() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let trash = root.path();
    for dir in ["files/folder/inner", "info"] {
        std::fs::create_dir_all(trash.join(dir)).expect("a trash folder");
    }
    std::fs::write(trash.join("files/a.txt"), "a").expect("a trashed file");
    std::fs::write(trash.join("files/folder/inner/b"), "b").expect("a nested file");
    std::fs::write(trash.join("info/a.txt.trashinfo"), "[Trash Info]").expect("a sidecar");
    std::fs::write(trash.join("directorysizes"), "").expect("a size cache");
    let outside = root.path().join("outside");
    std::fs::write(&outside, "keep").expect("a file outside the trash");
    std::os::unix::fs::symlink(&outside, trash.join("files/link")).expect("a symlink");

    empty_trash(trash).expect("emptied");

    for dir in ["files", "info"] {
        assert_eq!(
            std::fs::read_dir(trash.join(dir)).expect(dir).count(),
            0,
            "{dir} is emptied"
        );
    }
    assert!(!trash.join("directorysizes").exists());
    assert!(
        outside.exists(),
        "a trashed symlink is removed, never followed"
    );
}

#[tokio::test]
async fn the_state_before_any_event_has_arrived_is_empty_and_the_service_is_not_degraded() {
    let (_service, _ctx, state, health) = places(enabled()).await;

    assert_eq!(*state.borrow(), PlacesState::default());
    assert!(!matches!(
        &*health.borrow(),
        crate::ServiceState::Degraded { .. }
    ));
}
