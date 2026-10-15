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
#[derive(Debug, Clone, PartialEq, Eq, Type, Serialize, Deserialize)]
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
    pub paused_until: i64,
}

impl TryFrom<Value<'_>> for NightLightSnapshot {
    type Error = zbus::zvariant::Error;

    fn try_from(value: Value<'_>) -> Result<Self, Self::Error> {
        let mut fields = zbus::zvariant::Structure::try_from(value)?
            .into_fields()
            .into_iter();
        Ok(Self {
            schedule: fields
                .next()
                .ok_or(Self::Error::IncorrectType)?
                .try_into()?,
            overridden: fields
                .next()
                .ok_or(Self::Error::IncorrectType)?
                .try_into()?,
            temperature: fields
                .next()
                .ok_or(Self::Error::IncorrectType)?
                .try_into()?,
            target: fields
                .next()
                .ok_or(Self::Error::IncorrectType)?
                .try_into()?,
            active: fields
                .next()
                .ok_or(Self::Error::IncorrectType)?
                .try_into()?,
            serving: fields
                .next()
                .ok_or(Self::Error::IncorrectType)?
                .try_into()?,
            reason: fields
                .next()
                .ok_or(Self::Error::IncorrectType)?
                .try_into()?,
            configured: fields
                .next()
                .map(String::try_from)
                .transpose()?
                .unwrap_or_default(),
            manual: fields
                .next()
                .map(bool::try_from)
                .transpose()?
                .unwrap_or_default(),
            paused_until: fields
                .next()
                .map(i64::try_from)
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

impl TryFrom<OwnedValue> for NightLightSnapshot {
    type Error = zbus::zvariant::Error;

    fn try_from(value: OwnedValue) -> Result<Self, Self::Error> {
        Self::try_from(Value::from(value))
    }
}

impl From<NightLightSnapshot> for Value<'_> {
    fn from(snapshot: NightLightSnapshot) -> Self {
        Self::from((
            snapshot.schedule,
            snapshot.overridden,
            snapshot.temperature,
            snapshot.target,
            snapshot.active,
            snapshot.serving,
            snapshot.reason,
            snapshot.configured,
            snapshot.manual,
            snapshot.paused_until,
        ))
    }
}

impl TryFrom<NightLightSnapshot> for OwnedValue {
    type Error = zbus::zvariant::Error;

    fn try_from(snapshot: NightLightSnapshot) -> Result<Self, Self::Error> {
        Self::try_from(Value::from(snapshot))
    }
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

    fn pause(&self) -> zbus::Result<()>;

    fn resume(&self) -> zbus::Result<()>;
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
    #[error("night light is not active: {0}")]
    NotActive(String),
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

    pub async fn pause(&self) -> Result<(), NightLightProviderError> {
        call(self.proxy().await?.pause()).await
    }

    pub async fn resume(&self) -> Result<(), NightLightProviderError> {
        call(self.proxy().await?.resume()).await
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
                "me.aresa.Glimpse.NightLight1.Error.NotActive" => {
                    Err(NightLightProviderError::NotActive(reason))
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
            replace_unavailable(&updates, error.to_string());
            tracing::warn!(%error, "cannot observe the night light provider owner");
            return;
        }
    };
    let mut owners = match dbus
        .receive_name_owner_changed_with_args(&[(0, GLIMPSE_NIGHT_LIGHT_BUS_NAME)])
        .await
    {
        Ok(owners) => owners,
        Err(error) => {
            replace_unavailable(&updates, error.to_string());
            tracing::warn!(%error, "cannot observe night light provider owner changes");
            return;
        }
    };

    let mut retry_delay = super::RetryDelay::default();

    loop {
        let proxy = match NightLight1Proxy::builder(&connection)
            .cache_properties(CacheProperties::Yes)
            .build()
            .await
        {
            Ok(proxy) => {
                retry_delay.reset();
                proxy
            }
            Err(error) => {
                *current.write().await = None;
                replace_unavailable(&updates, error.to_string());
                tracing::debug!(
                    reason = %error,
                    "night light provider property cache is unavailable"
                );
                if !super::retry_after_failure(
                    &dbus,
                    &mut owners,
                    GLIMPSE_NIGHT_LIGHT_BUS_NAME,
                    &mut retry_delay,
                )
                .await
                {
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
                    replace_unavailable(&updates, error.to_string());
                    *current.write().await = None;
                    tracing::warn!(reason = %error, "night light snapshot is unavailable");
                    None
                }
            },
            Err(error) => {
                replace_unavailable(&updates, error.to_string());
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
                            replace_unavailable(&updates, error.to_string());
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
            replace_unavailable(&updates, &reason);
            tracing::warn!(reason, "night light provider disconnected");
        } else {
            tracing::debug!(reason, "night light provider rebuilding its proxy");
        }
    }
}

fn replace_unavailable(
    updates: &tokio::sync::watch::Sender<NightLightProviderState>,
    reason: impl Into<String>,
) {
    let next = NightLightProviderState::unavailable(reason);
    updates.send_if_modified(|previous| {
        if *previous == next {
            return false;
        }
        *previous = next;
        true
    });
}

#[cfg(test)]
mod tests;
