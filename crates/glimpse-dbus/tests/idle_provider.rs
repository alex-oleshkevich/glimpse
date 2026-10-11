use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use glimpse_dbus::idle::{
    BackendHealth, GLIMPSE_IDLE_BUS_NAME, GLIMPSE_IDLE_OBJECT_PATH, HealthKind,
    IdleInhibitorRecord, IdleInhibitorSource, IdleProvider, IdleProviderState, InhibitionTargets,
    InhibitorsHealth, Login1Mode,
};
use glimpse_dbus::testing::PrivateBus;
use tokio::sync::watch;
use zbus::Connection;

type Calls = Arc<Mutex<Vec<(&'static str, u64)>>>;
type Inhibitors = Arc<Mutex<Vec<IdleInhibitorRecord>>>;
type Health = Arc<Mutex<InhibitorsHealth>>;

struct TestProvider {
    calls: Calls,
    inhibitors: Inhibitors,
    health: Health,
}

fn ready() -> InhibitorsHealth {
    let health = BackendHealth {
        kind: HealthKind::Ready,
        message: String::new(),
    };
    InhibitorsHealth {
        screen_saver: health.clone(),
        portal: health.clone(),
        login1: health.clone(),
        wayland: health,
    }
}

fn one_record() -> IdleInhibitorRecord {
    IdleInhibitorRecord {
        id: 7,
        who: "Zoom".to_owned(),
        why: "screen sharing".to_owned(),
        bus_name: ":1.42".to_owned(),
        process_name: "zoom".to_owned(),
        source: IdleInhibitorSource::login1(555, 1000, Login1Mode::Block),
        targets: InhibitionTargets::manual_hold(),
        can_release: true,
        added_at_unix: 1_789_382_400,
    }
}

fn another_record() -> IdleInhibitorRecord {
    IdleInhibitorRecord {
        id: 8,
        who: "Firefox".to_owned(),
        why: "watching a video".to_owned(),
        bus_name: ":1.43".to_owned(),
        process_name: "firefox".to_owned(),
        source: IdleInhibitorSource::screen_saver(3),
        targets: InhibitionTargets::idle_only(),
        can_release: false,
        added_at_unix: 1_789_382_460,
    }
}

#[zbus::interface(name = "me.aresa.Glimpse.Idle1")]
impl TestProvider {
    #[zbus(property)]
    fn inhibitors(&self) -> Vec<IdleInhibitorRecord> {
        self.inhibitors.lock().unwrap().clone()
    }

    #[zbus(property)]
    fn health(&self) -> InhibitorsHealth {
        self.health.lock().unwrap().clone()
    }

    fn hold(&self, seconds: u32) -> u64 {
        self.calls
            .lock()
            .unwrap()
            .push(("hold", u64::from(seconds)));
        99
    }

    fn release(&self, id: u64) -> zbus::fdo::Result<()> {
        self.calls.lock().unwrap().push(("release", id));
        if id == 7 {
            Ok(())
        } else {
            Err(zbus::fdo::Error::UnknownObject("already gone".to_owned()))
        }
    }
}

async fn serve(
    bus: &PrivateBus,
    calls: Calls,
    inhibitors: Inhibitors,
    health: Health,
) -> Connection {
    let connection = bus.connection().await;
    connection
        .object_server()
        .at(
            GLIMPSE_IDLE_OBJECT_PATH,
            TestProvider {
                calls,
                inhibitors,
                health,
            },
        )
        .await
        .unwrap();
    glimpse_dbus::own_name(&connection, GLIMPSE_IDLE_BUS_NAME)
        .await
        .unwrap();
    connection
}

async fn emit_inhibitors_changed(connection: &Connection) {
    let iface = connection
        .object_server()
        .interface::<_, TestProvider>(GLIMPSE_IDLE_OBJECT_PATH)
        .await
        .unwrap();
    iface
        .get()
        .await
        .inhibitors_changed(iface.signal_emitter())
        .await
        .unwrap();
}

async fn emit_health_changed(connection: &Connection) {
    let iface = connection
        .object_server()
        .interface::<_, TestProvider>(GLIMPSE_IDLE_OBJECT_PATH)
        .await
        .unwrap();
    iface
        .get()
        .await
        .health_changed(iface.signal_emitter())
        .await
        .unwrap();
}

async fn wait_until(
    receiver: &mut watch::Receiver<IdleProviderState>,
    predicate: impl Fn(&IdleProviderState) -> bool,
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

struct ExpiryProvider {
    expiries: Arc<Mutex<BTreeMap<u64, u64>>>,
}

#[zbus::interface(name = "me.aresa.Glimpse.Idle1")]
impl ExpiryProvider {
    #[zbus(property)]
    fn inhibitors(&self) -> Vec<IdleInhibitorRecord> {
        vec![one_record()]
    }

    #[zbus(property)]
    fn health(&self) -> InhibitorsHealth {
        ready()
    }

    #[zbus(property)]
    fn hold_expiries(&self) -> HashMap<u64, u64> {
        self.expiries
            .lock()
            .unwrap()
            .iter()
            .map(|(id, expiry)| (*id, *expiry))
            .collect()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_decodes_and_follows_additive_hold_expiries() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    let expiries = Arc::new(Mutex::new(BTreeMap::from([(7, 1_789_386_000)])));
    server
        .object_server()
        .at(
            GLIMPSE_IDLE_OBJECT_PATH,
            ExpiryProvider {
                expiries: expiries.clone(),
            },
        )
        .await
        .unwrap();
    glimpse_dbus::own_name(&server, GLIMPSE_IDLE_BUS_NAME)
        .await
        .unwrap();
    let mut provider = IdleProvider::start(bus.connection().await);
    let mut updates = provider.handle().subscribe();
    wait_until(&mut updates, |state| state.available).await;
    assert_eq!(
        updates.borrow().hold_expiries,
        BTreeMap::from([(7, 1_789_386_000)])
    );
    assert_eq!(updates.borrow().inhibitors, [one_record()]);
    expiries.lock().unwrap().clear();
    let interface = server
        .object_server()
        .interface::<_, ExpiryProvider>(GLIMPSE_IDLE_OBJECT_PATH)
        .await
        .unwrap();
    interface
        .get()
        .await
        .hold_expiries_changed(interface.signal_emitter())
        .await
        .unwrap();
    wait_until(&mut updates, |state| state.hold_expiries.is_empty()).await;
    assert!(updates.borrow().available);
    provider.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_connects_after_the_client_and_recovers_after_owner_loss() {
    let mut bus = PrivateBus::start();
    let client = bus.connection().await;
    let mut provider = IdleProvider::start(client);
    let handle = provider.handle();
    let mut state = handle.subscribe();
    assert!(!handle.snapshot().owner);
    assert!(!handle.snapshot().available);

    let calls = Arc::new(Mutex::new(Vec::new()));
    let inhibitors = Arc::new(Mutex::new(vec![one_record()]));
    let health = Arc::new(Mutex::new(ready()));
    let first = serve(&bus, calls.clone(), inhibitors.clone(), health.clone()).await;
    wait_until(&mut state, |state| state.owner).await;
    let snapshot = handle.snapshot();
    assert!(snapshot.available);
    assert!(snapshot.hold_expiries.is_empty());
    assert_eq!(snapshot.inhibitors, vec![one_record()]);
    assert_eq!(snapshot.health, ready());

    let id = handle.hold(30).await.unwrap();
    assert_eq!(id, 99);
    handle.release(7).await.unwrap();
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[("hold", 30), ("release", 7)]
    );
    assert!(matches!(
        handle.release(99).await,
        Err(glimpse_dbus::idle::IdleProviderError::UnknownObject)
    ));

    inhibitors.lock().unwrap().push(another_record());
    emit_inhibitors_changed(&first).await;
    wait_until(&mut state, |state| state.inhibitors.len() == 2).await;
    assert_eq!(
        handle.snapshot().inhibitors,
        vec![one_record(), another_record()]
    );
    assert_eq!(
        handle.snapshot().health,
        ready(),
        "the paired health property must not change just because inhibitors did"
    );

    {
        let mut current = health.lock().unwrap();
        current.screen_saver = BackendHealth {
            kind: HealthKind::Degraded,
            message: "no compositor idle protocol".to_owned(),
        };
    }
    emit_health_changed(&first).await;
    wait_until(&mut state, |state| {
        state.health.screen_saver.kind == HealthKind::Degraded
    })
    .await;
    let updated = handle.snapshot();
    assert_eq!(
        updated.health.screen_saver.message,
        "no compositor idle protocol"
    );
    assert_eq!(
        updated.inhibitors,
        vec![one_record(), another_record()],
        "the paired inhibitor list must not change just because health did"
    );

    drop(first);
    wait_until(&mut state, |state| !state.owner).await;
    let disconnected = handle.snapshot();
    assert!(!disconnected.available);
    assert!(disconnected.inhibitors.is_empty());
    assert_eq!(disconnected.health, updated.health);
    assert!(handle.hold(10).await.is_err());

    let second = serve(&bus, calls.clone(), inhibitors.clone(), health.clone()).await;
    wait_until(&mut state, |state| state.owner).await;
    assert_eq!(
        handle.snapshot().inhibitors,
        vec![one_record(), another_record()]
    );

    bus.kill();
    wait_until(&mut state, |state| !state.owner).await;

    drop(second);
    provider.shutdown().await;
}

struct TransientProvider {
    attempts: Arc<std::sync::atomic::AtomicUsize>,
}

#[zbus::interface(name = "org.freedesktop.DBus.Properties")]
impl TransientProvider {
    fn get_all(&self, interface: &str) -> zbus::fdo::Result<HashMap<String, zbus::zvariant::OwnedValue>> {
        assert_eq!(interface, "me.aresa.Glimpse.Idle1");
        if self.attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            return Err(zbus::fdo::Error::Failed("transient GetAll failure".to_owned()));
        }
        Ok(HashMap::from([
            ("Inhibitors".to_owned(), zbus::zvariant::OwnedValue::try_from(zbus::zvariant::Value::from(vec![one_record()])).unwrap()),
            ("Health".to_owned(), zbus::zvariant::OwnedValue::try_from(ready()).unwrap()),
        ]))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_retries_get_all_failure_while_name_stays_owned() {
    let bus = PrivateBus::start();
    let server = serve(&bus, Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(vec![one_record()])), Arc::new(Mutex::new(ready()))).await;
    server.object_server().remove::<zbus::fdo::Properties, _>(GLIMPSE_IDLE_OBJECT_PATH).await.unwrap();
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    server.object_server().at(GLIMPSE_IDLE_OBJECT_PATH, TransientProvider {
        attempts: attempts.clone(),
    }).await.unwrap();
    let mut provider = IdleProvider::start(bus.connection().await);
    let mut updates = provider.handle().subscribe();
    wait_until(&mut updates, |state| state.available).await;
    assert_eq!(updates.borrow().inhibitors, vec![one_record()]);
    assert!(attempts.load(std::sync::atomic::Ordering::SeqCst) >= 2);
    provider.shutdown().await;
}
