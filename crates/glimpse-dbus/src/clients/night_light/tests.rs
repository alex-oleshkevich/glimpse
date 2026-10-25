use std::sync::{Arc as StdArc, Mutex};
use std::time::Duration;

use tokio::sync::watch;

use crate::clients::follower::replace_unavailable;
use crate::testing::PrivateBus;

use super::*;

#[test]
fn the_wire_signature_matches_the_versioned_contract() {
    assert_eq!(NightLightSnapshot::SIGNATURE, "(sbuubbssbxb)");
}

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Schedule(String),
    Temperature(u32),
    Pause,
    Resume,
}

type Calls = StdArc<Mutex<Vec<Call>>>;

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "me.aresa.Glimpse.NightLight1.Error", impl_display = true)]
enum TestError {
    NotActive(String),
    #[zbus(error)]
    ZBus(zbus::Error),
}

struct TestProvider {
    calls: Calls,
    temperature: u32,
}

#[zbus::interface(name = "me.aresa.Glimpse.NightLight1")]
impl TestProvider {
    #[zbus(property)]
    fn snapshot(&self) -> NightLightSnapshot {
        NightLightSnapshot {
            schedule: "automatic".to_owned(),
            overridden: false,
            temperature: self.temperature,
            target: 4000,
            active: false,
            serving: true,
            reason: String::new(),
            configured: "automatic".to_owned(),
            manual: false,
            paused_until: 0,
            pausable: false,
        }
    }

    fn pause(&self) -> Result<(), TestError> {
        if self.temperature == 0 {
            return Err(TestError::NotActive("off".to_owned()));
        }
        self.calls.lock().unwrap().push(Call::Pause);
        Ok(())
    }

    fn resume(&self) {
        self.calls.lock().unwrap().push(Call::Resume);
    }

    fn set_schedule(&self, schedule: &str) {
        self.calls
            .lock()
            .unwrap()
            .push(Call::Schedule(schedule.to_owned()));
    }

    fn set_temperature(&self, kelvin: u32) {
        self.calls.lock().unwrap().push(Call::Temperature(kelvin));
    }
}

async fn serve(bus: &PrivateBus, calls: Calls) -> zbus::Connection {
    let connection = bus.connection().await;
    connection
        .object_server()
        .at(
            GLIMPSE_NIGHT_LIGHT_OBJECT_PATH,
            TestProvider {
                calls,
                temperature: 6500,
            },
        )
        .await
        .unwrap();
    crate::own_name(&connection, GLIMPSE_NIGHT_LIGHT_BUS_NAME)
        .await
        .unwrap();
    connection
}

async fn wait_until(
    receiver: &mut watch::Receiver<NightLightProviderState>,
    predicate: impl Fn(&NightLightProviderState) -> bool,
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

#[tokio::test]
async fn unavailable_reports_the_reason_and_spawns_no_task() {
    let mut provider = NightLightProvider::unavailable("no bus in tests");
    assert!(provider.task.is_none());
    let handle = provider.handle();
    let state = handle.snapshot();
    assert_eq!(state.current, None);
    assert_eq!(state.unavailable.as_deref(), Some("no bus in tests"));
    assert!(handle.set_schedule("automatic").await.is_err());
    provider.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn start_before_the_provider_appears_reports_pending_then_the_resolved_loss() {
    let bus = PrivateBus::start();
    let connection = bus.connection().await;
    let mut provider = NightLightProvider::start(connection);
    let handle = provider.handle();
    let mut watching = handle.subscribe();
    wait_until(&mut watching, |state| {
        state.unavailable.as_deref() != Some(crate::clients::PENDING)
    })
    .await;
    let settled = handle.snapshot().unavailable.expect("still unavailable");
    assert_eq!(settled, "provider has no bus owner");

    let error = handle.set_temperature(4000).await.unwrap_err();
    assert!(matches!(error, NightLightProviderError::Unavailable(_)));
    provider.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_connects_after_the_client_and_recovers_after_owner_loss() {
    let mut bus = PrivateBus::start();
    let client = bus.connection().await;
    let mut provider = NightLightProvider::start(client);
    let handle = provider.handle();
    let mut state = handle.subscribe();
    assert!(handle.snapshot().unavailable.is_some());

    let calls: Calls = StdArc::new(Mutex::new(Vec::new()));
    let first = serve(&bus, calls.clone()).await;
    wait_until(&mut state, |state| state.unavailable.is_none()).await;
    handle.set_schedule("schedule").await.unwrap();
    handle.set_temperature(4500).await.unwrap();
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            Call::Schedule("schedule".to_owned()),
            Call::Temperature(4500)
        ]
    );
    assert_eq!(handle.snapshot().current.unwrap().schedule, "automatic");

    drop(first);
    wait_until(&mut state, |state| state.unavailable.is_some()).await;
    assert!(handle.set_temperature(4500).await.is_err());

    let second = serve(&bus, calls.clone()).await;
    wait_until(&mut state, |state| state.unavailable.is_none()).await;
    handle.set_schedule("off").await.unwrap();
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            Call::Schedule("schedule".to_owned()),
            Call::Temperature(4500),
            Call::Schedule("off".to_owned()),
        ]
    );

    bus.kill();
    wait_until(&mut state, |state| state.unavailable.is_some()).await;

    drop(second);
    provider.shutdown().await;
}

#[test]
fn seven_field_snapshot_defaults_additive_fields() {
    let value = OwnedValue::try_from(Value::from((
        "automatic",
        false,
        6500u32,
        4000u32,
        false,
        true,
        "",
    )))
    .unwrap();
    let snapshot = NightLightSnapshot::try_from(value).unwrap();
    assert_eq!(snapshot.schedule, "automatic");
    assert_eq!(snapshot.temperature, 6500);
    assert_eq!(snapshot.target, 4000);
    assert!(!snapshot.overridden);
    assert!(!snapshot.active);
    assert!(snapshot.serving);
    assert!(snapshot.reason.is_empty());
    assert!(snapshot.configured.is_empty());
    assert!(!snapshot.manual);
}

#[test]
fn malformed_snapshots_return_errors() {
    for value in [
        Value::from(("automatic", false)),
        Value::from(("automatic", false, "wrong", 4000u32, false, true, "")),
        Value::from(("automatic", false, 6500u32, 4000u32, false, true, "", false)),
        Value::from("not a structure"),
    ] {
        assert!(NightLightSnapshot::try_from(OwnedValue::try_from(value).unwrap()).is_err());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connected_client_observes_snapshot_changes() {
    let bus = PrivateBus::start();
    let server = serve(&bus, StdArc::new(Mutex::new(Vec::new()))).await;
    let mut provider = NightLightProvider::start(bus.connection().await);
    let mut updates = provider.handle().subscribe();
    wait_until(&mut updates, |state| state.current.is_some()).await;
    assert_eq!(updates.borrow().current.as_ref().unwrap().temperature, 6500);
    let interface = server
        .object_server()
        .interface::<_, TestProvider>(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH)
        .await
        .unwrap();
    interface.get_mut().await.temperature = 4500;
    interface
        .get()
        .await
        .snapshot_changed(interface.signal_emitter())
        .await
        .unwrap();
    wait_until(&mut updates, |state| {
        state
            .current
            .as_ref()
            .is_some_and(|snapshot| snapshot.temperature == 4500)
    })
    .await;
    assert!(updates.borrow().unavailable.is_none());
    provider.shutdown().await;
}

struct TransientProperties {
    attempts: StdArc<std::sync::atomic::AtomicUsize>,
}

#[zbus::interface(name = "org.freedesktop.DBus.Properties")]
impl TransientProperties {
    fn get_all(
        &self,
        interface: &str,
    ) -> zbus::fdo::Result<std::collections::HashMap<String, OwnedValue>> {
        if interface != "me.aresa.Glimpse.NightLight1" {
            return Err(zbus::fdo::Error::UnknownInterface(interface.to_owned()));
        }
        if self
            .attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            == 0
        {
            return Err(zbus::fdo::Error::Failed(
                "transient GetAll failure".to_owned(),
            ));
        }
        let snapshot = TestProvider {
            calls: StdArc::new(Mutex::new(Vec::new())),
            temperature: 4500,
        }
        .snapshot();
        let value = OwnedValue::try_from(Value::from(snapshot))
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
        Ok(std::collections::HashMap::from([(
            "Snapshot".to_owned(),
            value,
        )]))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_recovers_from_get_all_failure_while_name_stays_owned() {
    let bus = PrivateBus::start();
    let server = serve(&bus, StdArc::new(Mutex::new(Vec::new()))).await;
    server
        .object_server()
        .remove::<zbus::fdo::Properties, _>(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH)
        .await
        .unwrap();
    let attempts = StdArc::new(std::sync::atomic::AtomicUsize::new(0));
    server
        .object_server()
        .at(
            GLIMPSE_NIGHT_LIGHT_OBJECT_PATH,
            TransientProperties {
                attempts: attempts.clone(),
            },
        )
        .await
        .unwrap();
    let mut provider = NightLightProvider::start(bus.connection().await);
    let mut updates = provider.handle().subscribe();
    wait_until(&mut updates, |state| state.current.is_some()).await;
    assert_eq!(updates.borrow().current.as_ref().unwrap().temperature, 4500);
    assert!(attempts.load(std::sync::atomic::Ordering::SeqCst) >= 2);
    provider.shutdown().await;
}

#[test]
fn repeated_unavailability_does_not_notify_subscribers() {
    let (updates, mut receiver) = watch::channel(NightLightProviderState::unavailable("first"));
    replace_unavailable(&updates, "failed GetAll");
    assert!(receiver.has_changed().unwrap());
    receiver.borrow_and_update();
    replace_unavailable(&updates, "failed GetAll");
    assert!(!receiver.has_changed().unwrap());
}

#[test]
fn a_nine_field_snapshot_defaults_the_pause_and_a_new_one_round_trips() {
    let old = Value::from((
        "automatic",
        false,
        6500u32,
        4200u32,
        false,
        true,
        "",
        "automatic",
        false,
    ));
    let mut snapshot = NightLightSnapshot::try_from(old).unwrap();
    assert_eq!(snapshot.paused_until, 0);
    assert!(!snapshot.pausable);
    snapshot.paused_until = 1_800_000_000;
    snapshot.pausable = true;
    assert_eq!(
        NightLightSnapshot::try_from(Value::from(snapshot.clone())).unwrap(),
        snapshot
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_and_resume_are_sent_to_the_provider() {
    let bus = PrivateBus::start();
    let calls = StdArc::new(Mutex::new(Vec::new()));
    let _server = serve(&bus, calls.clone()).await;
    let mut provider = NightLightProvider::start(bus.connection().await);
    let handle = provider.handle();
    wait_until(&mut handle.subscribe(), |state| state.current.is_some()).await;
    handle.pause().await.unwrap();
    handle.resume().await.unwrap();
    assert_eq!(*calls.lock().unwrap(), [Call::Pause, Call::Resume]);
    provider.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_inactive_pause_is_decoded_as_not_active_over_a_private_bus() {
    let bus = PrivateBus::start();
    let server = serve(&bus, StdArc::new(Mutex::new(Vec::new()))).await;
    server
        .object_server()
        .interface::<_, TestProvider>(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH)
        .await
        .unwrap()
        .get_mut()
        .await
        .temperature = 0;
    let mut provider = NightLightProvider::start(bus.connection().await);
    let handle = provider.handle();
    wait_until(&mut handle.subscribe(), |state| state.current.is_some()).await;
    assert!(
        matches!(handle.pause().await, Err(NightLightProviderError::NotActive(reason)) if reason == "off")
    );
    provider.shutdown().await;
}

struct OldProvider;

#[zbus::interface(name = "me.aresa.Glimpse.NightLight1")]
impl OldProvider {
    #[zbus(property)]
    fn snapshot(&self) -> (String, bool, u32, u32, bool, bool, String, String, bool) {
        (
            "automatic".to_owned(),
            false,
            4200,
            4200,
            true,
            true,
            String::new(),
            "automatic".to_owned(),
            false,
        )
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_old_provider_with_nine_fields_connects_as_not_paused() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    server
        .object_server()
        .at(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH, OldProvider)
        .await
        .unwrap();
    crate::own_name(&server, GLIMPSE_NIGHT_LIGHT_BUS_NAME)
        .await
        .unwrap();
    let mut provider = NightLightProvider::start(bus.connection().await);
    let handle = provider.handle();
    wait_until(&mut handle.subscribe(), |state| state.current.is_some()).await;
    assert_eq!(handle.snapshot().current.unwrap().paused_until, 0);
    provider.shutdown().await;
}
