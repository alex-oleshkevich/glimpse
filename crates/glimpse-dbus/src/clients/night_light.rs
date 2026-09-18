use std::sync::Arc;

use futures_util::StreamExt;
use glimpse_utils::clean;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, watch};
use zbus::proxy::CacheProperties;
use zbus::zvariant::{OwnedValue, Type, Value};

pub const GLIMPSE_NIGHT_LIGHT_BUS_NAME: &str = "me.aresa.Glimpse.NightLight";
pub const GLIMPSE_NIGHT_LIGHT_OBJECT_PATH: &str = "/me/aresa/Glimpse/NightLight";

/// A struct rather than a positional tuple, so the field names survive onto the wire description
/// and a reader needs no comment to know which `u` is which. The signature is the same either way.
#[derive(Debug, Clone, PartialEq, Eq, Type, Value, OwnedValue, Serialize, Deserialize)]
pub struct NightLightSnapshot {
    /// `off`, `automatic` or `schedule`, spelled as `[night-light] schedule` spells it. This is
    /// the mode in force, which is the document's only while `overridden` is false.
    pub schedule: String,
    /// Whether `schedule` came from `SetSchedule` rather than from the document. An override lasts
    /// until `[night-light]` is edited or the process restarts; it is never written back.
    pub overridden: bool,
    /// The color temperature applied now, in kelvin. 6500 means nothing is applied.
    pub temperature: u32,
    /// The configured night temperature, in kelvin.
    pub target: u32,
    /// Whether `temperature` differs from neutral daylight.
    pub active: bool,
    /// Whether the provider is applying the schedule rather than reporting why it cannot.
    pub serving: bool,
    /// Why it is not serving, empty when it is.
    pub reason: String,
    pub configured: String,
    pub manual: bool,
}

#[zbus::proxy(
    interface = "me.aresa.Glimpse.NightLight1",
    default_service = "me.aresa.Glimpse.NightLight",
    default_path = "/me/aresa/Glimpse/NightLight"
)]
pub trait NightLight1 {
    #[zbus(property)]
    fn snapshot(&self) -> zbus::Result<NightLightSnapshot>;

    /// `off`, `automatic` or `schedule`. Runtime only: the document is untouched, and the next
    /// edit to `[night-light]` takes the mode back.
    fn set_schedule(&self, schedule: &str) -> zbus::Result<()>;

    fn set_temperature(&self, kelvin: u32) -> zbus::Result<()>;
}

const REASON: usize = 240;

#[derive(Debug, Clone, PartialEq)]
pub struct NightLightProviderState {
    pub current: Option<NightLightSnapshot>,
    pub unavailable: Option<String>,
}

impl NightLightProviderState {
    fn decoded(snapshot: NightLightSnapshot) -> Self {
        Self {
            current: Some(snapshot),
            unavailable: None,
        }
    }

    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            current: None,
            unavailable: Some(clean(&reason.into(), REASON)),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NightLightProviderError {
    #[error("night light schedule is invalid: {0}")]
    InvalidSchedule(String),
    #[error("night light temperature is invalid: {0}")]
    InvalidTemperature(String),
    #[error("night light provider unavailable: {0}")]
    Unavailable(String),
    #[error("night light provider call timed out")]
    TimedOut,
    #[error("night light provider call failed: {0}")]
    Call(String),
}

#[derive(Clone)]
pub struct NightLightProviderHandle {
    state: watch::Receiver<NightLightProviderState>,
    proxy: Arc<RwLock<Option<NightLight1Proxy<'static>>>>,
}

pub struct NightLightProvider {
    handle: NightLightProviderHandle,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl NightLightProvider {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let (_, state) = watch::channel(NightLightProviderState::unavailable(reason));
        Self {
            handle: NightLightProviderHandle {
                state,
                proxy: Default::default(),
            },
            task: None,
        }
    }

    pub fn start(connection: zbus::Connection) -> Self {
        let (updates, state) = watch::channel(NightLightProviderState::unavailable(
            "provider has no bus owner",
        ));
        let proxy = Arc::new(RwLock::new(None));
        let task = tokio::spawn(follow_provider(connection, updates, proxy.clone()));
        Self {
            handle: NightLightProviderHandle { state, proxy },
            task: Some(task),
        }
    }

    pub fn handle(&self) -> NightLightProviderHandle {
        self.handle.clone()
    }

    pub async fn shutdown(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
            tracing::debug!("night light provider follower stopped");
        }
    }
}

impl Drop for NightLightProvider {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

impl NightLightProviderHandle {
    pub fn snapshot(&self) -> NightLightProviderState {
        self.state.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<NightLightProviderState> {
        self.state.clone()
    }

    async fn proxy(&self) -> Result<NightLight1Proxy<'static>, NightLightProviderError> {
        self.proxy.read().await.clone().ok_or_else(|| {
            NightLightProviderError::Unavailable(
                self.state
                    .borrow()
                    .unavailable
                    .clone()
                    .unwrap_or_else(|| "provider has no bus owner".to_owned()),
            )
        })
    }

    pub async fn set_schedule(&self, schedule: &str) -> Result<(), NightLightProviderError> {
        call(self.proxy().await?.set_schedule(schedule)).await
    }

    pub async fn set_temperature(&self, kelvin: u32) -> Result<(), NightLightProviderError> {
        call(self.proxy().await?.set_temperature(kelvin)).await
    }
}

async fn call<T>(
    request: impl Future<Output = zbus::Result<T>>,
) -> Result<T, NightLightProviderError> {
    match tokio::time::timeout(super::DEADLINE, request).await {
        Ok(Ok(answer)) => Ok(answer),
        Ok(Err(zbus::Error::MethodError(name, reason, _))) => {
            let reason = clean(&reason.unwrap_or_default(), REASON);
            match name.as_str() {
                "me.aresa.Glimpse.NightLight1.Error.InvalidSchedule" => {
                    Err(NightLightProviderError::InvalidSchedule(reason))
                }
                "me.aresa.Glimpse.NightLight1.Error.InvalidTemperature" => {
                    Err(NightLightProviderError::InvalidTemperature(reason))
                }
                "me.aresa.Glimpse.NightLight1.Error.Unavailable"
                | "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner" => {
                    Err(NightLightProviderError::Unavailable(reason))
                }
                "org.freedesktop.DBus.Error.NoReply" => Err(NightLightProviderError::TimedOut),
                _ => Err(NightLightProviderError::Call(reason)),
            }
        }
        Ok(Err(error)) => Err(NightLightProviderError::Call(clean(
            &error.to_string(),
            REASON,
        ))),
        Err(_) => Err(NightLightProviderError::TimedOut),
    }
}

async fn follow_provider(
    connection: zbus::Connection,
    updates: watch::Sender<NightLightProviderState>,
    current: Arc<RwLock<Option<NightLight1Proxy<'static>>>>,
) {
    let dbus = match zbus::fdo::DBusProxy::new(&connection).await {
        Ok(dbus) => dbus,
        Err(error) => {
            updates.send_replace(NightLightProviderState::unavailable(error.to_string()));
            tracing::warn!(%error, "cannot observe the night light provider owner");
            return;
        }
    };
    let mut owners = match dbus.receive_name_owner_changed().await {
        Ok(owners) => owners,
        Err(error) => {
            updates.send_replace(NightLightProviderState::unavailable(error.to_string()));
            tracing::warn!(%error, "cannot observe night light provider owner changes");
            return;
        }
    };

    loop {
        let proxy = match NightLight1Proxy::builder(&connection)
            .cache_properties(CacheProperties::Yes)
            .build()
            .await
        {
            Ok(proxy) => proxy,
            Err(error) => {
                *current.write().await = None;
                updates.send_replace(NightLightProviderState::unavailable(error.to_string()));
                tracing::debug!(
                    reason = %error,
                    "night light provider has no owner; waiting"
                );
                if !wait_for_owner(&mut owners).await {
                    return;
                }
                continue;
            }
        };
        let mut snapshots = proxy.receive_snapshot_changed().await;
        let snapshot = match proxy.cached_snapshot() {
            Ok(Some(snapshot)) => Some(snapshot),
            Ok(None) => match proxy.snapshot().await {
                Ok(snapshot) => Some(snapshot),
                Err(error) => {
                    updates.send_replace(NightLightProviderState::unavailable(error.to_string()));
                    *current.write().await = None;
                    tracing::warn!(reason = %error, "night light snapshot is unavailable");
                    None
                }
            },
            Err(error) => {
                updates.send_replace(NightLightProviderState::unavailable(error.to_string()));
                *current.write().await = None;
                tracing::warn!(reason = %error, "night light property cache is unavailable");
                None
            }
        };
        let mut serving = match snapshot {
            Some(snapshot) => {
                *current.write().await = Some(proxy.clone());
                let state = NightLightProviderState::decoded(snapshot);
                tracing::info!(
                    serving = state
                        .current
                        .as_ref()
                        .is_some_and(|snapshot| snapshot.serving),
                    "night light provider connected"
                );
                updates.send_replace(state);
                true
            }
            None => false,
        };

        let reason = loop {
            tokio::select! {
                changed = snapshots.next() => {
                    let Some(changed) = changed else {
                        break "night light snapshot stream ended".to_owned();
                    };
                    match changed.get().await {
                        Ok(snapshot) => {
                            if !serving {
                                *current.write().await = Some(proxy.clone());
                                serving = true;
                                tracing::info!("night light provider recovered without changing owner");
                            }
                            let state = NightLightProviderState::decoded(snapshot);
                            tracing::debug!(
                                serving = state.current.as_ref().is_some_and(|snapshot| snapshot.serving),
                                "night light snapshot changed"
                            );
                            updates.send_replace(state);
                        }
                        Err(error) => {
                            updates.send_replace(NightLightProviderState::unavailable(error.to_string()));
                            tracing::warn!(reason = %error, "night light snapshot change failed");
                            if serving {
                                *current.write().await = None;
                                serving = false;
                            }
                        }
                    }
                }
                owner = owners.next() => {
                    let Some(owner) = owner else {
                        break "night light provider owner stream ended".to_owned();
                    };
                    let Ok(args) = owner.args() else {
                        continue;
                    };
                    if args.name().as_str() != GLIMPSE_NIGHT_LIGHT_BUS_NAME {
                        continue;
                    }
                    if args.old_owner().is_some() {
                        break match args.new_owner().is_some() {
                            true => "provider owner changed",
                            false => "provider has no bus owner",
                        }.to_owned();
                    }
                    if !serving && args.new_owner().is_some() {
                        break "provider took the name".to_owned();
                    }
                }
            }
        };
        if serving {
            *current.write().await = None;
            updates.send_replace(NightLightProviderState::unavailable(&reason));
            tracing::warn!(reason, "night light provider disconnected");
        } else {
            tracing::debug!(reason, "night light provider rebuilding its proxy");
        }
    }
}

async fn wait_for_owner(owners: &mut zbus::fdo::NameOwnerChangedStream) -> bool {
    while let Some(owner) = owners.next().await {
        let Ok(args) = owner.args() else {
            continue;
        };
        if args.name().as_str() == GLIMPSE_NIGHT_LIGHT_BUS_NAME && args.new_owner().is_some() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
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
    }

    #[zbus::interface(name = "me.aresa.Glimpse.NightLight1")]
    impl TestProvider {
        #[zbus(property)]
        fn snapshot(&self) -> NightLightSnapshot {
            NightLightSnapshot {
                schedule: "automatic".to_owned(),
                overridden: false,
                temperature: 6500,
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
            .at(GLIMPSE_NIGHT_LIGHT_OBJECT_PATH, TestProvider { calls })
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
}
