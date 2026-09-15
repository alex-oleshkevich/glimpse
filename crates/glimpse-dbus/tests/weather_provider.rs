mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use glimpse_dbus::weather::WatchedPlace;
use glimpse_dbus::weather::{
    GLIMPSE_WEATHER_BUS_NAME, GLIMPSE_WEATHER_OBJECT_PATH, PlaceWeatherWire, WeatherProvider,
    WeatherProviderState, WeatherSnapshot,
};
use support::PrivateBus;
use tokio::sync::watch;
use zbus::Connection;

type Calls = Arc<Mutex<Vec<(u8, f64, f64, String)>>>;

struct TestProvider {
    calls: Calls,
}

#[zbus::interface(name = "me.aresa.Glimpse.Weather1")]
impl TestProvider {
    #[zbus(property)]
    fn snapshot(&self) -> WeatherSnapshot {
        let place: PlaceWeatherWire = (
            0,
            0.0,
            0.0,
            String::new(),
            54.7,
            25.3,
            "Vilnius".to_owned(),
            "LT".to_owned(),
            7_200,
            (
                false,
                (
                    0,
                    255,
                    false,
                    0.0,
                    (false, 0.0),
                    (false, 0),
                    (false, 0.0),
                    (false, 0),
                    (false, 0.0),
                ),
            ),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        (
            true,
            false,
            String::new(),
            1_789_382_400_000_000,
            0,
            vec![place],
        )
    }

    fn watch_place(&self, kind: u8, latitude: f64, longitude: f64, location: &str) {
        self.calls
            .lock()
            .unwrap()
            .push((kind, latitude, longitude, location.to_owned()));
    }

    fn refresh(&self) {}
}

async fn serve(bus: &PrivateBus, calls: Calls) -> Connection {
    let connection = bus.connection().await;
    connection
        .object_server()
        .at(GLIMPSE_WEATHER_OBJECT_PATH, TestProvider { calls })
        .await
        .unwrap();
    glimpse_dbus::own_name(&connection, GLIMPSE_WEATHER_BUS_NAME)
        .await
        .unwrap();
    connection
}

async fn wait_until(
    receiver: &mut watch::Receiver<WeatherProviderState>,
    predicate: impl Fn(&WeatherProviderState) -> bool,
) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if predicate(&receiver.borrow()) {
                return;
            }
            receiver.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_connects_after_the_client_and_recovers_after_owner_loss() {
    let mut bus = PrivateBus::start();
    let client = bus.connection().await;
    let mut provider = WeatherProvider::start(client);
    let handle = provider.handle();
    let mut state = handle.subscribe();
    assert!(!handle.snapshot().owner);

    let calls = Arc::new(Mutex::new(Vec::new()));
    let first = serve(&bus, calls.clone()).await;
    wait_until(&mut state, |state| state.owner).await;
    handle.watch(WatchedPlace::Here).await.unwrap();
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[(0, 0.0, 0.0, String::new())]
    );
    let snapshot = handle.snapshot();
    let place = &snapshot.status.unwrap().places[0];
    assert_eq!(place.city.as_deref(), Some("Vilnius"));
    assert_eq!(place.country_code.as_deref(), Some("LT"));

    drop(first);
    wait_until(&mut state, |state| !state.owner).await;
    let disconnected = handle.snapshot();
    assert!(disconnected.stale);
    assert_eq!(disconnected.status.unwrap().places.len(), 1);
    assert!(handle.watch(WatchedPlace::Here).await.is_err());

    let second = serve(&bus, calls.clone()).await;
    wait_until(&mut state, |state| state.owner).await;
    handle
        .watch(WatchedPlace::Coordinates {
            latitude: 54.7,
            longitude: 25.3,
        })
        .await
        .unwrap();
    handle
        .watch(WatchedPlace::Location {
            name: "Warsaw, PL".to_owned(),
        })
        .await
        .unwrap();
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            (0, 0.0, 0.0, String::new()),
            (1, 54.7, 25.3, String::new()),
            (2, 0.0, 0.0, "Warsaw, PL".to_owned()),
        ]
    );

    let _ = bus.child.kill();
    let _ = bus.child.wait();
    wait_until(&mut state, |state| !state.owner).await;

    drop(second);
    provider.shutdown().await;
}
