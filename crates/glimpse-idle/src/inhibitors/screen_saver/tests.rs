use std::time::Duration;

use glimpse_dbus::testing::PrivateBus;

use super::*;

#[zbus::proxy(
    interface = "org.freedesktop.ScreenSaver",
    default_service = "org.freedesktop.ScreenSaver",
    default_path = "/ScreenSaver"
)]
trait ScreenSaver {
    fn inhibit(&self, application_name: &str, reason_for_inhibit: &str) -> zbus::Result<u32>;
    fn un_inhibit(&self, cookie: u32) -> zbus::Result<()>;
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(50)).await;
}

async fn start_server(bus: &PrivateBus) -> (Connection, Arc<SharedRegistry>) {
    let app = bus.connection().await;
    let (registry, _any_idle_target, _generation) = SharedRegistry::new();
    let health = Health::new().0;
    start(app.clone(), registry.clone(), health).await;
    (app, registry)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inhibit_at_either_path_returns_a_cookie_with_idle_only_targets() {
    let bus = PrivateBus::start();
    let (_app, registry) = start_server(&bus).await;

    for path in [OBJECT_PATH, LEGACY_OBJECT_PATH] {
        let client = bus.connection().await;
        let proxy = ScreenSaverProxy::builder(&client)
            .path(path)
            .unwrap()
            .build()
            .await
            .unwrap();

        let cookie = proxy.inhibit("test", "smoke").await.unwrap();
        assert_ne!(cookie, 0);

        let found = registry
            .read(|registry| {
                registry
                    .snapshot()
                    .iter()
                    .any(|record| record.source.cookie == cookie && record.targets.idle)
            })
            .await;
        assert!(found, "no idle-targeting record for path {path}");
    }
}
#[tokio::test]
async fn un_inhibit_of_an_unknown_cookie_succeeds() {
    let bus = PrivateBus::start();
    let (_app, _registry) = start_server(&bus).await;
    let client = bus.connection().await;
    let proxy = ScreenSaverProxy::new(&client).await.unwrap();

    proxy.un_inhibit(999_999).await.unwrap();
}

#[tokio::test]
async fn un_inhibit_cannot_release_another_clients_cookie() {
    let bus = PrivateBus::start();
    let (_app, registry) = start_server(&bus).await;
    let owner = bus.connection().await;
    let owner_proxy = ScreenSaverProxy::new(&owner).await.unwrap();
    let cookie = owner_proxy.inhibit("test", "smoke").await.unwrap();
    let other = bus.connection().await;
    let other_proxy = ScreenSaverProxy::new(&other).await.unwrap();

    other_proxy.un_inhibit(cookie).await.unwrap();
    assert!(
        registry
            .read(|registry| registry.lookup_by_cookie(cookie).is_some())
            .await
    );

    owner_proxy.un_inhibit(cookie).await.unwrap();
    assert!(
        !registry
            .read(|registry| registry.lookup_by_cookie(cookie).is_some())
            .await
    );
}

#[tokio::test]
async fn un_inhibit_is_never_rate_limited() {
    let bus = PrivateBus::start();
    let (_app, registry) = start_server(&bus).await;
    let client = bus.connection().await;
    let proxy = ScreenSaverProxy::new(&client).await.unwrap();

    for _ in 0..8 {
        proxy.un_inhibit(999_999).await.expect("a release is free");
    }

    let cookie = proxy.inhibit("player", "playing").await.unwrap();
    for _ in 0..8 {
        let _ = proxy.inhibit("player", "playing").await;
    }
    proxy.un_inhibit(cookie).await.expect(
        "a player that inhibits on play and releases on pause exhausts the bucket;              refusing the release strands the record and suppresses every idle listener",
    );
    assert!(
        registry
            .read(|registry| registry.lookup_by_cookie(cookie))
            .await
            .is_none()
    );
}

#[tokio::test]
async fn inhibit_clamps_oversized_multi_byte_labels_to_their_own_caps() {
    let bus = PrivateBus::start();
    let (_app, registry) = start_server(&bus).await;
    let client = bus.connection().await;
    let proxy = ScreenSaverProxy::new(&client).await.unwrap();

    let who_input = "é".repeat(200);
    let why_input = "é".repeat(300);
    let cookie = proxy.inhibit(&who_input, &why_input).await.unwrap();

    let (who, why) = registry
        .read(|registry| {
            let record = registry
                .snapshot()
                .into_iter()
                .find(|record| record.source.cookie == cookie)
                .unwrap();
            (record.who, record.why)
        })
        .await;
    assert!(who.chars().count() <= WHO_CAP + 1, "who: {who:?}");
    assert!(why.chars().count() <= WHY_CAP + 1, "why: {why:?}");
    assert!(who.ends_with('…'));
    assert!(why.ends_with('…'));
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disconnecting_client_auto_releases_its_records() {
    let bus = PrivateBus::start();
    let (app, registry) = start_server(&bus).await;
    let cancel = CancellationToken::new();
    let watch_task = tokio::spawn(watch_disconnects(
        app.clone(),
        registry.clone(),
        cancel.clone(),
    ));

    let client = bus.connection().await;
    let proxy = ScreenSaverProxy::new(&client).await.unwrap();
    let cookie = proxy.inhibit("test", "smoke").await.unwrap();
    assert!(
        registry
            .read(|registry| registry.lookup_by_cookie(cookie).is_some())
            .await
    );

    drop(proxy);
    drop(client);
    settle().await;

    assert!(
        !registry
            .read(|registry| registry.lookup_by_cookie(cookie).is_some())
            .await,
        "a disconnected client's record must be auto-released"
    );

    cancel.cancel();
    let _ = watch_task.await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn process_name_backfills_after_the_initial_reply() {
    let bus = PrivateBus::start();
    let (_app, registry) = start_server(&bus).await;
    let client = bus.connection().await;
    let proxy = ScreenSaverProxy::new(&client).await.unwrap();

    let cookie = proxy.inhibit("test", "smoke").await.unwrap();

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let process_name = registry
                .read(|registry| {
                    registry
                        .snapshot()
                        .into_iter()
                        .find(|record| record.source.cookie == cookie)
                        .map(|record| record.process_name)
                })
                .await;
            if process_name.as_deref().is_some_and(|name| !name.is_empty()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("process_name to backfill");
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inhibit_reports_capacity_rejection_as_limits_exceeded() {
    let bus = PrivateBus::start();
    let (_app, registry) = start_server(&bus).await;
    registry
        .mutate(|registry| {
            for i in 0..super::super::Registry::MAX_INHIBITORS_TOTAL {
                let id = registry.mint_id();
                let record = IdleInhibitorRecord {
                    id,
                    who: "who".into(),
                    why: "why".into(),
                    bus_name: format!(":1.filler.{i}"),
                    process_name: String::new(),
                    source: IdleInhibitorSource::screen_saver(0),
                    targets: InhibitionTargets::idle_only(),
                    can_release: true,
                    added_at_unix: 0,
                };
                registry.insert(record, None);
            }
        })
        .await;

    let client = bus.connection().await;
    let proxy = ScreenSaverProxy::new(&client).await.unwrap();
    let error = proxy.inhibit("test", "smoke").await.unwrap_err();
    assert!(
        matches!(
            &error,
            zbus::Error::MethodError(name, _, _)
                if name.as_str() == "org.freedesktop.DBus.Error.LimitsExceeded"
        ),
        "unexpected error: {error:?}"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_acquisition_attempt_degrades_health_but_keeps_serving() {
    let bus = PrivateBus::start();
    let first = bus.connection().await;
    let (first_registry, _any_idle_target, _generation) = SharedRegistry::new();
    let first_health = Health::new().0;
    start(first.clone(), first_registry.clone(), first_health.clone()).await;
    assert_eq!(
        first_health.kind(Backend::ScreenSaver),
        glimpse_dbus::idle::HealthKind::Ready
    );

    let second = bus.connection().await;
    let (second_registry, _any_idle_target, _generation) = SharedRegistry::new();
    let second_health = Health::new().0;
    start(
        second.clone(),
        second_registry.clone(),
        second_health.clone(),
    )
    .await;

    let health = second_health.snapshot();
    assert_eq!(
        health.screen_saver.kind,
        glimpse_dbus::idle::HealthKind::Degraded
    );
    assert_eq!(health.screen_saver.message, "Bus name already owned");

    assert!(
        second
            .object_server()
            .interface::<_, ScreenSaverServer>(OBJECT_PATH)
            .await
            .is_ok(),
        "the un-owning second server must still register its own object"
    );

    let client = bus.connection().await;
    let proxy = ScreenSaverProxy::new(&client).await.unwrap();
    let cookie = proxy.inhibit("test", "smoke").await.unwrap();
    assert!(
        first_registry
            .read(|registry| registry.lookup_by_cookie(cookie).is_some())
            .await,
        "the first, name-owning server must keep serving Inhibit"
    );
}
