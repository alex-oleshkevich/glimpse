use std::time::Duration;

use glimpse_config::{Config, Geolocation as ConfiguredGeolocation};
use glimpse_dbus::Buses;
use glimpse_dbus::geoclue::GeoClueManagerProxy;
use glimpse_dbus::testing::PrivateBus;
use glimpse_dbus::testing::geoclue::{Call, FakeGeoClue};
use glimpse_dbus::weather::GeoCoordinates;
use glimpse_services::{Geolocation, GeolocationHandle, GeolocationStatus, Running, ServiceState};
use zbus::zvariant::OwnedObjectPath;

async fn geolocation(
    bus: &PrivateBus,
    document: &Config,
) -> (Running<Geolocation>, GeolocationHandle) {
    let connection = bus.connection().await;
    let buses = Buses::for_system(connection);
    Running::<Geolocation>::spawn(document, buses, ())
}

fn manual(latitude: f64, longitude: f64) -> Config {
    Config {
        geolocation: ConfiguredGeolocation::Manual {
            latitude,
            longitude,
        },
        ..Default::default()
    }
}

fn first_client(calls: &[Call]) -> OwnedObjectPath {
    calls
        .iter()
        .find_map(|call| match call {
            Call::Start(path) => Some(path.clone()),
            _ => None,
        })
        .expect("a Start call carrying the client path")
}

fn last_client(calls: &[Call]) -> OwnedObjectPath {
    calls
        .iter()
        .rev()
        .find_map(|call| match call {
            Call::Start(path) => Some(path.clone()),
            _ => None,
        })
        .expect("a Start call carrying the client path")
}

async fn calls_until(
    fake: &FakeGeoClue,
    bound: Duration,
    ready: impl Fn(&[Call]) -> bool,
) -> Vec<Call> {
    tokio::time::timeout(bound, async {
        loop {
            let calls = fake.calls();
            if ready(&calls) {
                return calls;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "the call log never reached the expected state; saw {:?}",
            fake.calls()
        )
    })
}

async fn state_until(
    handle: &GeolocationHandle,
    bound: Duration,
    ready: impl Fn(&GeolocationStatus) -> bool,
) -> GeolocationStatus {
    let mut state = handle.subscribe();
    tokio::time::timeout(bound, async {
        loop {
            if ready(&state.borrow_and_update()) {
                return;
            }
            state.changed().await.expect("the service is alive");
        }
    })
    .await
    .unwrap_or_else(|_| panic!("state never settled; last seen {:?}", handle.snapshot()));
    handle.snapshot()
}

async fn health_until(
    handle: &GeolocationHandle,
    bound: Duration,
    ready: impl Fn(&ServiceState) -> bool,
) -> ServiceState {
    let mut health = handle.health();
    tokio::time::timeout(bound, async {
        loop {
            if ready(&health.borrow_and_update()) {
                return;
            }
            health.changed().await.expect("the service is alive");
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "health never settled; last seen {:?}",
            *handle.health().borrow()
        )
    });
    handle.health().borrow().clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fix_arriving_releases_the_client_and_clears_in_use() {
    let bus = PrivateBus::start();
    let fake = FakeGeoClue::start(bus.connection().await).await.unwrap();
    let (mut service, handle) = geolocation(&bus, &Config::default()).await;

    let calls = calls_until(&fake, Duration::from_secs(5), |calls| {
        calls.iter().any(|call| matches!(call, Call::Start(_)))
    })
    .await;
    let client = first_client(&calls);
    assert!(fake.in_use(), "a started client must be reflected honestly");

    let fix = GeoCoordinates {
        latitude: 48.8566,
        longitude: 2.3522,
    };
    fake.push_fix(&client, fix.latitude, fix.longitude)
        .await
        .unwrap();

    state_until(&handle, Duration::from_secs(5), |state| {
        state.coordinates == Some(fix.clone())
    })
    .await;

    // The release runs synchronously inside the same attempt that decodes the fix, before the
    // event ever reaches the published state, so both calls are already in the log here.
    let calls = fake.calls();
    let stop_at = calls
        .iter()
        .position(|call| call == &Call::Stop(client.clone()));
    let delete_at = calls
        .iter()
        .position(|call| call == &Call::DeleteClient(client.clone()));
    assert!(stop_at.is_some(), "Stop must land: {calls:?}");
    assert!(delete_at.is_some(), "DeleteClient must land: {calls:?}");
    assert!(
        stop_at.unwrap() < delete_at.unwrap(),
        "Stop must precede DeleteClient: {calls:?}"
    );
    assert!(
        !fake.in_use(),
        "InUse must go false once the client that pinned it is released"
    );

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fix_that_never_arrives_is_still_released_after_the_bounded_timeout() {
    let bus = PrivateBus::start();
    let fake = FakeGeoClue::start(bus.connection().await).await.unwrap();
    let (mut service, handle) = geolocation(&bus, &Config::default()).await;

    let calls = calls_until(&fake, Duration::from_secs(5), |calls| {
        calls.iter().any(|call| matches!(call, Call::Start(_)))
    })
    .await;
    let client = first_client(&calls);
    assert!(fake.in_use());

    // No `push_fix`: production bounds the wait at `FIX_TIMEOUT` (30s) and releases the client
    // anyway, so this genuinely costs the wait rather than asserting a duration.
    calls_until(&fake, Duration::from_secs(40), |calls| {
        calls.iter().any(|call| call == &Call::Stop(client.clone()))
    })
    .await;
    calls_until(&fake, Duration::from_secs(5), |calls| {
        calls
            .iter()
            .any(|call| call == &Call::DeleteClient(client.clone()))
    })
    .await;
    assert!(
        !fake.in_use(),
        "a client that never got a fix must not pin InUse forever"
    );
    assert_eq!(
        handle.snapshot().coordinates,
        None,
        "a fix that never arrived must not be reported as one"
    );

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_refresh_after_release_is_handed_a_new_client_not_the_released_one() {
    let bus = PrivateBus::start();
    let fake = FakeGeoClue::start(bus.connection().await).await.unwrap();
    let (mut service, handle) = geolocation(&bus, &Config::default()).await;

    let calls = calls_until(&fake, Duration::from_secs(5), |calls| {
        calls.iter().any(|call| matches!(call, Call::Start(_)))
    })
    .await;
    let first = first_client(&calls);

    let fix = GeoCoordinates {
        latitude: 41.9028,
        longitude: 12.4964,
    };
    fake.push_fix(&first, fix.latitude, fix.longitude)
        .await
        .unwrap();
    state_until(&handle, Duration::from_secs(5), |state| {
        state.coordinates == Some(fix.clone())
    })
    .await;
    calls_until(&fake, Duration::from_secs(5), |calls| {
        calls
            .iter()
            .any(|call| call == &Call::DeleteClient(first.clone()))
    })
    .await;

    // Driven through the same command a `geolocation.refresh` call uses, never a wait on the
    // real 45s retry cadence.
    handle
        .refresh()
        .await
        .expect("refresh reaches the running service");

    let calls = calls_until(&fake, Duration::from_secs(5), |calls| {
        calls
            .iter()
            .filter(|call| matches!(call, Call::Start(_)))
            .count()
            >= 2
    })
    .await;
    let second = last_client(&calls);

    assert_ne!(
        first, second,
        "a retry must be handed a new client, not the released one"
    );

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_config_switch_mid_wait_still_releases_the_client() {
    let bus = PrivateBus::start();
    let fake = FakeGeoClue::start(bus.connection().await).await.unwrap();
    let (mut service, handle) = geolocation(&bus, &Config::default()).await;

    let calls = calls_until(&fake, Duration::from_secs(5), |calls| {
        calls.iter().any(|call| matches!(call, Call::Start(_)))
    })
    .await;
    let client = first_client(&calls);
    assert!(fake.in_use());

    // Switching to `manual` mid-wait removes the Geoclue subscription, which aborts the source
    // while it is still parked in `first_fix` — the same abort `Live::reconcile` performs.
    let target = manual(51.5074, -0.1278);
    service.reconfigure(&target);

    calls_until(&fake, Duration::from_secs(5), |calls| {
        calls.iter().any(|call| call == &Call::Stop(client.clone()))
    })
    .await;
    calls_until(&fake, Duration::from_secs(5), |calls| {
        calls
            .iter()
            .any(|call| call == &Call::DeleteClient(client.clone()))
    })
    .await;
    assert!(
        !fake.in_use(),
        "the abandoned client must not keep InUse pinned"
    );

    state_until(&handle, Duration::from_secs(5), |state| {
        state.coordinates
            == Some(GeoCoordinates {
                latitude: 51.5074,
                longitude: -0.1278,
            })
    })
    .await;

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_release_that_races_an_already_deleted_client_is_stepped_over() {
    let bus = PrivateBus::start();
    let fake = FakeGeoClue::start(bus.connection().await).await.unwrap();
    let (mut service, handle) = geolocation(&bus, &Config::default()).await;

    let calls = calls_until(&fake, Duration::from_secs(5), |calls| {
        calls.iter().any(|call| matches!(call, Call::Start(_)))
    })
    .await;
    let client = first_client(&calls);

    // Another actor releases the same client first, the way a second glimpse process or a
    // races-with-itself retry would.
    let racer = bus.connection().await;
    let manager = GeoClueManagerProxy::new(&racer).await.unwrap();
    manager.delete_client(client.clone()).await.unwrap();
    calls_until(&fake, Duration::from_secs(5), |calls| {
        calls
            .iter()
            .any(|call| call == &Call::DeleteClient(client.clone()))
    })
    .await;
    assert!(
        !matches!(&*handle.health().borrow(), ServiceState::Degraded { .. }),
        "a delete that lands before the service's own release must not degrade it on its own"
    );

    // Force the service's own release of the now-vanished client, the same seam AC-4 uses.
    let target = manual(40.4168, -3.7038);
    service.reconfigure(&target);

    let state = state_until(&handle, Duration::from_secs(5), |state| {
        state.coordinates.is_some()
    })
    .await;
    assert_eq!(
        state.coordinates,
        Some(GeoCoordinates {
            latitude: 40.4168,
            longitude: -3.7038,
        })
    );

    let settled = health_until(&handle, Duration::from_secs(5), |state| {
        !matches!(state, ServiceState::Starting)
    })
    .await;
    assert!(
        !matches!(settled, ServiceState::Degraded { .. }),
        "a release racing a client already gone must be stepped over, not reported as a \
         failure: {settled:?}"
    );

    // The abandoned source's release is a detached task racing this assertion, not something
    // the config-switch reply waits for, so its own arrival needs its own bounded wait.
    let calls = calls_until(&fake, Duration::from_secs(5), |calls| {
        calls
            .iter()
            .filter(|call| call == &&Call::DeleteClient(client.clone()))
            .count()
            >= 2
    })
    .await;
    assert!(
        !calls.iter().any(|call| call == &Call::Stop(client.clone())),
        "Stop must never reach an object already removed from the bus: {calls:?}"
    );

    service.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_transient_property_read_failure_leaves_the_cached_fix_untouched() {
    let bus = PrivateBus::start();
    let fake = FakeGeoClue::start(bus.connection().await).await.unwrap();
    let (mut service, handle) = geolocation(&bus, &Config::default()).await;

    let calls = calls_until(&fake, Duration::from_secs(5), |calls| {
        calls.iter().any(|call| matches!(call, Call::Start(_)))
    })
    .await;
    let first = first_client(&calls);

    let cached = GeoCoordinates {
        latitude: 35.6762,
        longitude: 139.6503,
    };
    fake.push_fix(&first, cached.latitude, cached.longitude)
        .await
        .unwrap();
    state_until(&handle, Duration::from_secs(5), |state| {
        state.coordinates == Some(cached.clone())
    })
    .await;

    handle
        .refresh()
        .await
        .expect("refresh reaches the running service");
    let calls = calls_until(&fake, Duration::from_secs(5), |calls| {
        calls
            .iter()
            .filter(|call| matches!(call, Call::Start(_)))
            .count()
            >= 2
    })
    .await;
    let second = last_client(&calls);

    let mut changes = handle.subscribe();
    changes.borrow_and_update();
    fake.fail_location_reads(true);
    fake.push_fix(&second, 0.0, 0.0).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(400), changes.changed())
            .await
            .is_err(),
        "a failed property read must not publish a wrong fix"
    );
    assert_eq!(
        handle.snapshot().coordinates,
        Some(cached),
        "the last known fix must survive a transient read failure"
    );

    fake.fail_location_reads(false);
    let real = GeoCoordinates {
        latitude: 35.6895,
        longitude: 139.6917,
    };
    fake.push_fix(&second, real.latitude, real.longitude)
        .await
        .unwrap();
    state_until(&handle, Duration::from_secs(5), |state| {
        state.coordinates == Some(real.clone())
    })
    .await;

    service.stop().await;
}
