use super::*;
use crate::testing::PrivateBus;

#[tokio::test(start_paused = true)]
async fn retries_double_cap_reset_and_wake_on_owner_changes() {
    let mut owners = futures_util::stream::pending::<()>();
    let mut delay = RetryDelay::default();
    for milliseconds in [250, 500, 1000, 2000, 4000, 8000, 16000, 30000, 30000] {
        let before = tokio::time::Instant::now();
        assert!(delay.wait(&mut owners).await);
        assert_eq!(before.elapsed(), Duration::from_millis(milliseconds));
    }
    delay.reset();
    let before = tokio::time::Instant::now();
    assert!(delay.wait(&mut owners).await);
    assert_eq!(before.elapsed(), Duration::from_millis(250));
    let mut changed = futures_util::stream::iter([()]);
    let before = tokio::time::Instant::now();
    assert!(delay.wait(&mut changed).await);
    assert_eq!(before.elapsed(), Duration::ZERO);
    assert!(!delay.wait(&mut changed).await);
}

#[tokio::test]
async fn an_unowned_provider_parks_without_backoff() {
    let bus = PrivateBus::start();
    let connection = bus.connection().await;
    let name = "me.aresa.Glimpse.RetryTest";
    let dbus = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
    let mut owners = owners(&dbus, name).await.unwrap();
    let mut delay = RetryDelay::default();
    let (retried, owned) = tokio::join!(
        retry_after_failure(&dbus, &mut owners, name, &mut delay),
        crate::own_name(&connection, name)
    );
    owned.unwrap();
    assert!(retried);
    assert_eq!(delay.0, Duration::from_millis(250));
}

#[tokio::test]
async fn the_first_resolution_with_no_owner_differs_from_the_seed() {
    let bus = PrivateBus::start();
    let mut provider = night_light::NightLightProvider::start(bus.connection().await);
    let handle = provider.handle();
    let mut state = handle.subscribe();
    let seed = state.borrow_and_update().unavailable.clone();
    assert_eq!(seed.as_deref(), Some(PENDING));
    let error = handle.set_temperature(4000).await.unwrap_err();
    match error {
        night_light::NightLightProviderError::Unavailable(reason) => {
            assert!(reason.contains(PENDING), "{reason}");
        }
        other => panic!("a call during the seed must be unavailable, got {other:?}"),
    }

    let resolved = tokio::time::timeout(
        Duration::from_secs(10),
        state.wait_for(|state| state.unavailable.as_deref() != Some(PENDING)),
    )
    .await
    .expect("the follower resolves")
    .expect("the sender lives")
    .unavailable
    .clone();

    assert!(resolved.is_some_and(|reason| !reason.is_empty()));
    provider.shutdown().await;
}

#[tokio::test]
async fn the_first_resolution_with_no_owner_is_the_no_owner_reason() {
    let bus = PrivateBus::start();
    let mut provider = night_light::NightLightProvider::start(bus.connection().await);
    let mut state = provider.handle().subscribe();
    assert_eq!(
        state.borrow_and_update().unavailable.as_deref(),
        Some(PENDING)
    );

    let resolved = tokio::time::timeout(
        Duration::from_secs(10),
        state.wait_for(|state| state.unavailable.as_deref() != Some(PENDING)),
    )
    .await
    .expect("the follower resolves")
    .expect("the sender lives")
    .unavailable
    .clone();

    assert_eq!(resolved.as_deref(), Some("provider has no bus owner"));
    provider.shutdown().await;
}

#[tokio::test]
async fn an_unowned_activatable_provider_is_activated() {
    let bus = PrivateBus::start();
    let marker = bus.service_dir().join("activated");
    std::fs::write(
        bus.service_dir().join("night-light.service"),
        format!(
            "[D-BUS Service]\nName={}\nExec=/bin/sh -c 'touch {}'\n",
            night_light::GLIMPSE_NIGHT_LIGHT_BUS_NAME,
            marker.display()
        ),
    )
    .unwrap();
    let connection = bus.connection().await;
    let dbus = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
    dbus.reload_config().await.unwrap();

    let mut provider = night_light::NightLightProvider::start(connection);
    let activated = tokio::time::timeout(Duration::from_secs(10), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;

    assert!(activated.is_ok(), "the follower never requested activation");
    provider.shutdown().await;
}
