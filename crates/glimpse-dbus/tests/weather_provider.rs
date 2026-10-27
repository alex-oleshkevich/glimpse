use std::sync::{Arc, Mutex};
use std::time::Duration;

use glimpse_dbus::testing::PrivateBus;
use glimpse_dbus::weather::WatchedPlace;
use glimpse_dbus::weather::{
    GLIMPSE_WEATHER_BUS_NAME, GLIMPSE_WEATHER_OBJECT_PATH, PlaceWeatherWire, WeatherProvider,
    WeatherProviderState, WeatherSnapshot,
};
use tokio::sync::watch;
use zbus::Connection;

type Calls = Arc<Mutex<Vec<(u8, f64, f64, String)>>>;

struct TestProvider {
    calls: Calls,
    city: String,
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
            self.city.clone(),
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
        .at(
            GLIMPSE_WEATHER_OBJECT_PATH,
            TestProvider {
                calls,
                city: "Vilnius".to_owned(),
            },
        )
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
    bus.kill();
    wait_until(&mut state, |state| !state.owner).await;

    drop(second);
    provider.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connected_client_observes_snapshot_changes() {
    let bus = PrivateBus::start();
    let server = serve(&bus, Arc::new(Mutex::new(Vec::new()))).await;
    let mut provider = WeatherProvider::start(bus.connection().await);
    let mut updates = provider.handle().subscribe();
    wait_until(&mut updates, |state| state.owner).await;
    assert_eq!(
        updates.borrow().status.as_ref().unwrap().places[0]
            .city
            .as_deref(),
        Some("Vilnius")
    );
    let interface = server
        .object_server()
        .interface::<_, TestProvider>(GLIMPSE_WEATHER_OBJECT_PATH)
        .await
        .unwrap();
    interface.get_mut().await.city = "Warsaw".to_owned();
    interface
        .get()
        .await
        .snapshot_changed(interface.signal_emitter())
        .await
        .unwrap();
    wait_until(&mut updates, |state| {
        state
            .status
            .as_ref()
            .is_some_and(|status| status.places[0].city.as_deref() == Some("Warsaw"))
    })
    .await;
    assert!(updates.borrow().owner);
    provider.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lazy_activation_does_not_restart_a_stopped_provider() {
    use futures_util::StreamExt;
    use std::io::Write;

    const ACTIVATED: &str = "GLIMPSE_TEST_WEATHER_ACTIVATION";
    if let Ok(counter) = std::env::var(ACTIVATED) {
        writeln!(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(counter)
                .unwrap(),
            "{}",
            std::process::id()
        )
        .unwrap();
        let address = std::env::var("DBUS_STARTER_ADDRESS").unwrap();
        let connection = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        connection
            .object_server()
            .at(
                GLIMPSE_WEATHER_OBJECT_PATH,
                TestProvider {
                    calls: Arc::new(Mutex::new(Vec::new())),
                    city: "Vilnius".to_owned(),
                },
            )
            .await
            .unwrap();
        glimpse_dbus::own_name(&connection, GLIMPSE_WEATHER_BUS_NAME)
            .await
            .unwrap();
        let mut messages = zbus::MessageStream::from(&connection);
        while messages.next().await.is_some() {}
        return;
    }

    let bus = PrivateBus::start();
    let socket = bus
        .address()
        .split(',')
        .next()
        .unwrap()
        .strip_prefix("unix:path=")
        .unwrap();
    let scratch = std::path::Path::new(socket).parent().unwrap();
    let counter = scratch.join("activations");
    let executable = std::env::current_exe().unwrap();
    std::fs::write(
        scratch.join("services/me.aresa.Glimpse.Weather.service"),
        format!(
            "[D-BUS Service]\nName={GLIMPSE_WEATHER_BUS_NAME}\nExec=/usr/bin/env {ACTIVATED}={} {} --exact lazy_activation_does_not_restart_a_stopped_provider --nocapture\n",
            counter.display(),
            executable.display()
        ),
    )
    .unwrap();
    let connection = bus.connection().await;
    zbus::fdo::DBusProxy::new(&connection)
        .await
        .unwrap()
        .reload_config()
        .await
        .unwrap();
    let mut provider = WeatherProvider::start(connection);
    let handle = provider.handle();
    let mut updates = handle.subscribe();
    wait_until(&mut updates, |state| state.owner).await;
    handle.watch(WatchedPlace::Here).await.unwrap();
    assert_eq!(
        handle.snapshot().status.unwrap().places[0].city.as_deref(),
        Some("Vilnius")
    );
    let activations = std::fs::read_to_string(&counter).unwrap();
    assert_eq!(activations.lines().count(), 1);
    let pid = activations.trim();
    assert!(
        std::process::Command::new("kill")
            .args(["-TERM", pid])
            .status()
            .unwrap()
            .success()
    );
    wait_until(&mut updates, |state| !state.owner).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        std::fs::read_to_string(&counter).unwrap().lines().count(),
        1,
        "owner loss must not activate another provider"
    );
    assert!(handle.watch(WatchedPlace::Here).await.is_err());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server = serve(&bus, calls.clone()).await;
    wait_until(&mut updates, |state| state.owner).await;
    handle.watch(WatchedPlace::Here).await.unwrap();
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[(0, 0.0, 0.0, String::new())]
    );
    provider.shutdown().await;
    drop(server);
}
