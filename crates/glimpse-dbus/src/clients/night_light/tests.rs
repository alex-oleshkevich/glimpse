use std::sync::{Arc as StdArc, Mutex};
use std::time::Duration;

use crate::testing::PrivateBus;

use super::*;

#[test]
fn the_wire_signature_matches_the_versioned_contract() {
    assert_eq!(NightLightSnapshot::SIGNATURE, "(sbuubbssb)");
}

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Schedule(String),
    Temperature(u32),
}

type Calls = StdArc<Mutex<Vec<Call>>>;

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
        }
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
        .at(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH, TestProvider { calls, temperature: 6500 })
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
async fn start_before_the_provider_appears_reports_no_bus_owner() {
    let bus = PrivateBus::start();
    let connection = bus.connection().await;
    let mut provider = NightLightProvider::start(connection);
    let handle = provider.handle();
    let state = handle.snapshot();
    assert_eq!(state.current, None);
    assert_eq!(
        state.unavailable.as_deref(),
        Some("provider has no bus owner")
    );

    let mut watching = handle.subscribe();
    wait_until(&mut watching, |state| {
        state.unavailable.as_deref() != Some("provider has no bus owner")
    })
    .await;
    let settled = handle.snapshot().unavailable.expect("still unavailable");
    assert!(!settled.is_empty());
    assert!(
        settled.contains(GLIMPSE_NIGHT_LIGHT_BUS_NAME),
        "the settled reason should name the bus name the panel could not reach, got {settled}"
    );

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
        "automatic", false, 6500u32, 4000u32, false, true, "",
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
    let interface = server.object_server().interface::<_, TestProvider>(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH).await.unwrap();
    interface.get_mut().await.temperature = 4500;
    interface.get().await.snapshot_changed(interface.signal_emitter()).await.unwrap();
    wait_until(&mut updates, |state| state.current.as_ref().is_some_and(|snapshot| snapshot.temperature == 4500)).await;
    assert!(updates.borrow().unavailable.is_none());
    provider.shutdown().await;
}
