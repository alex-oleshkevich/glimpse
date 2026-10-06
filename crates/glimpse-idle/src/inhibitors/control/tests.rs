use glimpse_dbus::idle::{
    GLIMPSE_IDLE_BUS_NAME, GLIMPSE_IDLE_OBJECT_PATH, HealthKind, Idle1Proxy,
    IdleInhibitorSource as TestSource, InhibitionTargets as TestTargets,
};
use glimpse_dbus::testing::PrivateBus;
use zbus::Connection;

use super::*;
use crate::inhibitors::Backend;
use crate::inhibitors::test_support::{FakeLogin1, start_fake_login1, start_paused_fake_login1};

async fn start_server(
    bus: &PrivateBus,
) -> (Connection, Connection, Arc<SharedRegistry>, FakeLogin1) {
    let (login1_conn, fake_login1) = start_fake_login1(bus).await;
    let app = bus.connection().await;
    let (registry, _any_idle_target, _generation) = SharedRegistry::new();
    let login1 = Login1ManagerProxy::new(&app).await.unwrap();
    let server = Idle1Server::new(registry.clone(), Some(login1), Health::new().0);
    app.object_server()
        .at(GLIMPSE_IDLE_OBJECT_PATH, server)
        .await
        .unwrap();
    glimpse_dbus::own_name(&app, GLIMPSE_IDLE_BUS_NAME)
        .await
        .unwrap();
    (login1_conn, app, registry, fake_login1)
}

async fn start_server_with_paused_login1(
    bus: &PrivateBus,
) -> (
    Connection,
    Connection,
    Arc<SharedRegistry>,
    FakeLogin1,
    Arc<tokio::sync::Notify>,
    Arc<tokio::sync::Notify>,
) {
    let (login1_conn, fake_login1, started, release) = start_paused_fake_login1(bus).await;
    let app = bus.connection().await;
    let (registry, _any_idle_target, _generation) = SharedRegistry::new();
    let login1 = Login1ManagerProxy::new(&app).await.unwrap();
    let server = Idle1Server::new(registry.clone(), Some(login1), Health::new().0);
    app.object_server()
        .at(GLIMPSE_IDLE_OBJECT_PATH, server)
        .await
        .unwrap();
    glimpse_dbus::own_name(&app, GLIMPSE_IDLE_BUS_NAME)
        .await
        .unwrap();
    (login1_conn, app, registry, fake_login1, started, release)
}
fn assert_fd_closed(fake_login1: &FakeLogin1, index: usize) {
    use std::io::Read as _;
    let mut readers = fake_login1.readers.lock().unwrap();
    let mut buf = [0u8; 1];
    let read = readers[index]
        .read(&mut buf)
        .expect("read from the fake login1 pipe");
    assert_eq!(
        read, 0,
        "EOF only appears once every fd referencing the write end is closed"
    );
}
async fn wait_until_released(registry: &SharedRegistry, id: u64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let present = registry
                .read(|registry| registry.snapshot().iter().any(|record| record.id == id))
                .await;
            if !present {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("expected inhibitor {id} to have auto-released"));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hold_returns_an_id_with_manual_hold_targets_and_auto_releases() {
    let bus = PrivateBus::start();
    let (_login1_conn, _app, registry, fake_login1) = start_server(&bus).await;
    let client = bus.connection().await;
    let proxy = Idle1Proxy::new(&client).await.unwrap();

    let id = proxy.hold(1).await.unwrap();
    assert_ne!(id, 0, "0 is a sentinel, never a real Hold id");

    let inhibitors = proxy.inhibitors().await.unwrap();
    let record = inhibitors.iter().find(|record| record.id == id).unwrap();
    assert_eq!(record.targets, TestTargets::manual_hold());
    assert!(record.can_release);
    assert!(matches!(
        record.source.kind,
        glimpse_dbus::idle::SourceKind::Login1
    ));
    assert_eq!(record.who, "glimpse-idle");
    assert_eq!(record.why, "Manual hold");
    let _indefinite = proxy.hold(0).await.unwrap();
    assert_eq!(
        proxy.hold_expiries().await.unwrap(),
        HashMap::from([(id, record.added_at_unix + 1)])
    );

    wait_until_released(&registry, id).await;
    assert!(
        registry
            .read(|registry| registry.hold_expiries())
            .await
            .is_empty()
    );
    assert_fd_closed(&fake_login1, 0);
}
#[tokio::test(start_paused = true)]
async fn hold_zero_seconds_never_auto_releases_only_release_clears_it() {
    let bus = PrivateBus::start();
    let (_login1_conn, _app, registry, _fake_login1) = start_server(&bus).await;
    let client = bus.connection().await;
    let proxy = Idle1Proxy::new(&client).await.unwrap();

    let id = proxy.hold(0).await.unwrap();

    assert!(proxy.hold_expiries().await.unwrap().is_empty());

    tokio::time::advance(Duration::from_secs(3600)).await;
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    let present = registry
        .read(|registry| registry.snapshot().iter().any(|record| record.id == id))
        .await;
    assert!(present, "Hold(0) must never auto-release");

    proxy.release(id).await.unwrap();
    let present = registry
        .read(|registry| registry.snapshot().iter().any(|record| record.id == id))
        .await;
    assert!(!present, "an explicit Release must clear a Hold(0) record");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hold_rechecks_capacity_after_logind_returns_and_closes_its_fd() {
    let bus = PrivateBus::start();
    let (_login1_conn, _app, registry, fake_login1, started, release) =
        start_server_with_paused_login1(&bus).await;
    let client = bus.connection().await;
    let proxy = Idle1Proxy::new(&client).await.unwrap();

    let hold = tokio::spawn(async move { proxy.hold(0).await });
    started.notified().await;
    registry
        .mutate(|registry| {
            for _ in 0..crate::inhibitors::Registry::MAX_INHIBITORS_TOTAL {
                let id = registry.mint_id();
                registry.insert(
                    IdleInhibitorRecord {
                        id,
                        who: "test".to_owned(),
                        why: String::new(),
                        bus_name: String::new(),
                        process_name: String::new(),
                        source: TestSource::login1(0, 0, Login1Mode::Block),
                        targets: TestTargets::NONE,
                        can_release: false,
                        added_at_unix: 0,
                    },
                    None,
                );
            }
        })
        .await;
    release.notify_one();

    let error = hold.await.unwrap().unwrap_err();
    assert!(
        matches!(
            &error,
            zbus::Error::MethodError(name, _, _)
                if name.as_str() == "org.freedesktop.DBus.Error.LimitsExceeded"
        ),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        registry.read(|registry| registry.snapshot().len()).await,
        crate::inhibitors::Registry::MAX_INHIBITORS_TOTAL
    );
    assert_fd_closed(&fake_login1, 0);
}
#[tokio::test(start_paused = true)]
async fn release_of_a_non_releasable_record_returns_not_supported() {
    let bus = PrivateBus::start();
    let (_login1_conn, _app, registry, _fake_login1) = start_server(&bus).await;
    let client = bus.connection().await;
    let proxy = Idle1Proxy::new(&client).await.unwrap();

    let id = registry
        .mutate(|registry| {
            let id = registry.mint_id();
            let record = glimpse_dbus::idle::IdleInhibitorRecord {
                id,
                who: "sleep.target".to_owned(),
                why: "systemd-observed".to_owned(),
                bus_name: String::new(),
                process_name: String::new(),
                source: TestSource::login1(1, 0, glimpse_dbus::idle::Login1Mode::Block),
                targets: TestTargets::idle_only(),
                can_release: false,
                added_at_unix: 0,
            };
            registry.insert(record, None)
        })
        .await;

    let error = proxy.release(id).await.unwrap_err();
    assert!(
        matches!(
            &error,
            zbus::Error::MethodError(name, _, _)
                if name.as_str() == "org.freedesktop.DBus.Error.NotSupported"
        ),
        "unexpected error: {error:?}"
    );

    let present = registry
        .read(|registry| registry.snapshot().iter().any(|record| record.id == id))
        .await;
    assert!(present, "a NotSupported release must not remove the record");
}
#[tokio::test(start_paused = true)]
async fn release_of_an_unknown_id_is_unknown_object() {
    let bus = PrivateBus::start();
    let (_login1_conn, _app, _registry, _fake_login1) = start_server(&bus).await;
    let client = bus.connection().await;
    let proxy = Idle1Proxy::new(&client).await.unwrap();

    let error = proxy.release(999_999).await.unwrap_err();
    assert!(
        matches!(
            &error,
            zbus::Error::MethodError(name, _, _)
                if name.as_str() == "org.freedesktop.DBus.Error.UnknownObject"
        ),
        "unexpected error: {error:?}"
    );
}
#[tokio::test(start_paused = true)]
async fn hold_without_login1_returns_not_supported() {
    let bus = PrivateBus::start();
    let app = bus.connection().await;
    let (registry, _any_idle_target, _generation) = SharedRegistry::new();
    let server = Idle1Server::new(registry, None, Health::new().0);
    app.object_server()
        .at(GLIMPSE_IDLE_OBJECT_PATH, server)
        .await
        .unwrap();
    glimpse_dbus::own_name(&app, GLIMPSE_IDLE_BUS_NAME)
        .await
        .unwrap();

    let client = bus.connection().await;
    let proxy = Idle1Proxy::new(&client).await.unwrap();

    let error = proxy.hold(1).await.unwrap_err();
    assert!(
        matches!(
            &error,
            zbus::Error::MethodError(name, _, _)
                if name.as_str() == "org.freedesktop.DBus.Error.NotSupported"
        ),
        "unexpected error: {error:?}"
    );
    assert!(proxy.inhibitors().await.unwrap().is_empty());
}
#[tokio::test]
async fn health_reports_every_backend_from_the_one_shared_value() {
    let (registry, _any_idle_target, _generation) = SharedRegistry::new();
    let health = Health::new().0;
    let server = Idle1Server::new(registry, None, health.clone());

    let initial = server.health().await;
    for kind in [
        initial.screen_saver.kind,
        initial.portal.kind,
        initial.login1.kind,
        initial.wayland.kind,
    ] {
        assert_eq!(kind, HealthKind::Unsupported, "nothing has reported yet");
    }

    health.degraded(Backend::ScreenSaver, "Bus name already owned");
    health.degraded(Backend::Portal, "Bus name already owned");
    health.degraded(Backend::Login1, "cannot reach org.freedesktop.login1");
    health.degraded(Backend::Wayland, "no ext-idle-notify-v1");

    let reported = server.health().await;
    assert_eq!(reported.screen_saver.message, "Bus name already owned");
    assert_eq!(reported.portal.message, "Bus name already owned");
    assert_eq!(
        reported.login1.message,
        "cannot reach org.freedesktop.login1"
    );
    assert_eq!(
        reported.wayland.message, "no ext-idle-notify-v1",
        "without this slot the compositor lacking the protocol is invisible to every client"
    );
}
