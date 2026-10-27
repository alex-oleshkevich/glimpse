use super::*;
use crate::testing::PrivateBus;
use follower::{Follow, Snapshots};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Notify, watch};
use weather::{Weather1Proxy, WeatherProviderError, WeatherProviderState, WeatherSnapshot};

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
async fn a_present_owner_is_returned_at_once_until_its_build_fails() {
    let bus = PrivateBus::start();
    let connection = bus.connection().await;
    let name = "me.aresa.Glimpse.RebuildTest";
    crate::own_name(&connection, name).await.unwrap();
    let dbus = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
    let mut owners = owners(&dbus, name).await.unwrap();
    let mut delay = RetryDelay::default();

    let owner = provider_owner(&dbus, &mut owners, name, false, &mut delay)
        .await
        .unwrap();
    assert_eq!(owner.as_ref(), connection.unique_name());
    assert_eq!(delay.0, Duration::from_millis(250));

    delay.reset();
    let owner = provider_owner(&dbus, &mut owners, name, false, &mut delay)
        .await
        .unwrap();
    assert_eq!(owner.as_ref(), connection.unique_name());
    assert_eq!(delay.0, Duration::from_millis(250));

    let owner = provider_owner(&dbus, &mut owners, name, false, &mut delay)
        .await
        .unwrap();
    assert_eq!(owner.as_ref(), connection.unique_name());
    assert_eq!(
        delay.0,
        Duration::from_millis(500),
        "a rebuild after a failed build backs off first"
    );
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
        provider_owner(&dbus, &mut owners, name, false, &mut delay),
        crate::own_name(&connection, name)
    );
    owned.unwrap();
    assert_eq!(retried.unwrap().as_ref(), connection.unique_name());
    assert_eq!(delay.0, Duration::from_millis(250));
}

async fn an_unowned_wake_waits_for_the_current_owner(wake: Delivery<(bool, bool)>) {
    let bus = PrivateBus::start();
    let connection = bus.connection().await;
    let name = "me.aresa.Glimpse.RetryWakeTest";
    let dbus = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
    let mut owners = futures_util::stream::iter([wake])
        .chain(owners(&dbus, name).await.unwrap())
        .boxed();
    let mut delay = RetryDelay::default();
    let mut retry = Box::pin(provider_owner(&dbus, &mut owners, name, false, &mut delay));
    assert!(
        tokio::time::timeout(Duration::from_millis(50), retry.as_mut())
            .await
            .is_err(),
        "a queued wake must not stand in for the current owner"
    );
    crate::own_name(&connection, name).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), retry)
            .await
            .unwrap()
            .unwrap()
            .as_ref(),
        connection.unique_name()
    );
}

#[tokio::test]
async fn a_queued_owner_gain_does_not_admit_an_unowned_provider() {
    an_unowned_wake_waits_for_the_current_owner(Delivery::Item((false, true))).await;
}

#[tokio::test]
async fn a_missed_owner_event_does_not_admit_an_unowned_provider() {
    an_unowned_wake_waits_for_the_current_owner(Delivery::Missed).await;
}

struct ActivationWeather;

#[zbus::interface(name = "me.aresa.Glimpse.Weather1")]
impl ActivationWeather {
    #[zbus(property)]
    fn snapshot(&self) -> WeatherSnapshot {
        (true, false, String::new(), 0, 0, Vec::new())
    }

    fn refresh(&self) {}
}

async fn weather_server(connection: zbus::Connection) -> zbus::Connection {
    connection
        .object_server()
        .at(weather::GLIMPSE_WEATHER_OBJECT_PATH, ActivationWeather)
        .await
        .unwrap();
    crate::own_name(&connection, weather::GLIMPSE_WEATHER_BUS_NAME)
        .await
        .unwrap();
    connection
}

async fn activated_weather_process() -> bool {
    use std::io::Write;

    let Ok(counter) = std::env::var("GLIMPSE_TEST_OWNER_ACTIVATION") else {
        return false;
    };
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
    let connection = weather_server(
        zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap(),
    )
    .await;
    let mut messages = zbus::MessageStream::from(&connection);
    while messages.next().await.is_some() {}
    true
}

async fn activation_bus(test: &str) -> (PrivateBus, PathBuf) {
    let bus = PrivateBus::start();
    let socket = bus
        .address()
        .split(',')
        .next()
        .unwrap()
        .strip_prefix("unix:path=")
        .unwrap();
    let scratch = Path::new(socket).parent().unwrap();
    let counter = scratch.join("activations");
    std::fs::write(
        scratch.join("services/me.aresa.Glimpse.Weather.service"),
        format!(
            "[D-BUS Service]\nName={}\nExec=/usr/bin/env GLIMPSE_TEST_OWNER_ACTIVATION={} {} --exact clients::tests::{test} --nocapture\n",
            weather::GLIMPSE_WEATHER_BUS_NAME,
            counter.display(),
            std::env::current_exe().unwrap().display()
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
    (bus, counter)
}

fn stop_activated_weather(counter: &Path) {
    let activations = std::fs::read_to_string(counter).unwrap();
    assert_eq!(activations.lines().count(), 1);
    assert!(
        std::process::Command::new("kill")
            .args(["-TERM", activations.trim()])
            .status()
            .unwrap()
            .success()
    );
}

async fn provider_state<S: Clone>(updates: &mut watch::Receiver<S>, matches: impl Fn(&S) -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !matches(&updates.borrow()) {
            updates.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retained_proxy_cannot_reactivate_its_stopped_owner() {
    if activated_weather_process().await {
        return;
    }
    let (bus, counter) =
        activation_bus("a_retained_proxy_cannot_reactivate_its_stopped_owner").await;
    let mut provider = weather::WeatherProvider::start(bus.connection().await);
    let handle = provider.handle();
    let mut updates = handle.subscribe();
    provider_state(&mut updates, |state| state.owner).await;
    let proxy = handle.proxy().await.unwrap();
    stop_activated_weather(&counter);
    provider_state(&mut updates, |state| !state.owner).await;
    assert!(proxy.refresh().await.is_err());
    assert_eq!(std::fs::read_to_string(counter).unwrap().lines().count(), 1);
    provider.shutdown().await;
}

#[derive(Default)]
struct BuildGate {
    builds: AtomicUsize,
    arrived: Notify,
    released: Notify,
    finished: Notify,
}

static BUILD_GATE: LazyLock<BuildGate> = LazyLock::new(BuildGate::default);

#[derive(Clone, PartialEq)]
struct GatedWeather(WeatherProviderState);

impl Follow for GatedWeather {
    type Proxy = Weather1Proxy<'static>;
    type Error = WeatherProviderError;

    const NAME: &'static str = "gated weather";

    fn lost(reason: String, previous: Option<&Self>) -> Self {
        Self(WeatherProviderState::lost(
            reason,
            previous.map(|state| &state.0),
        ))
    }

    fn reason(&self) -> Option<&str> {
        self.0.reason()
    }
}

impl Snapshots for GatedWeather {
    type Snapshot = WeatherSnapshot;

    const BUS_NAME: &'static str = weather::GLIMPSE_WEATHER_BUS_NAME;

    async fn build(
        connection: &zbus::Connection,
        owner: zbus::names::OwnedUniqueName,
    ) -> zbus::Result<Self::Proxy> {
        let gate = &*BUILD_GATE;
        let delayed = gate.builds.fetch_add(1, Ordering::SeqCst) == 1;
        if delayed {
            gate.arrived.notify_one();
            gate.released.notified().await;
        }
        let proxy = WeatherProviderState::build(connection, owner).await;
        if delayed {
            if let Ok(proxy) = &proxy {
                let _ = WeatherProviderState::read(proxy).await;
            }
            gate.finished.notify_one();
        }
        proxy
    }

    async fn read(proxy: &Self::Proxy) -> zbus::Result<Self::Snapshot> {
        WeatherProviderState::read(proxy).await
    }

    async fn changes(proxy: &Self::Proxy) -> follower::Changes<Self::Snapshot> {
        WeatherProviderState::changes(proxy).await
    }

    fn decode(snapshot: Self::Snapshot) -> Result<Self, String> {
        WeatherProviderState::decode(snapshot).map(Self)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gated_proxy_build_cannot_reactivate_a_disappeared_owner() {
    if activated_weather_process().await {
        return;
    }
    let gate = &*BUILD_GATE;
    let (bus, counter) =
        activation_bus("a_gated_proxy_build_cannot_reactivate_a_disappeared_owner").await;
    let mut provider = follower::Provider::<GatedWeather>::start(bus.connection().await);
    let handle = provider.handle();
    let mut updates = handle.subscribe();
    provider_state(&mut updates, |state| state.0.owner).await;
    stop_activated_weather(&counter);
    provider_state(&mut updates, |state| !state.0.owner).await;
    let replacement = weather_server(bus.connection().await).await;
    tokio::time::timeout(Duration::from_secs(3), gate.arrived.notified())
        .await
        .unwrap();
    drop(replacement);
    gate.released.notify_one();
    tokio::time::timeout(Duration::from_secs(3), gate.finished.notified())
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&counter).unwrap().lines().count(),
        1,
        "a vanished owner must not turn the delayed cache read into activation"
    );
    let recovered = weather_server(bus.connection().await).await;
    provider_state(&mut updates, |state| state.0.owner).await;
    handle.proxy().await.unwrap().refresh().await.unwrap();
    provider.shutdown().await;
    drop(recovered);
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
