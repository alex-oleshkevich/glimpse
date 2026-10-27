use serde::{Deserialize, Serialize};
use zbus::proxy::CacheProperties;
use zbus::zvariant::{OwnedValue, Type, Value};

use super::follower::{
    Changes, Follow, Provider, ProviderError, ProviderHandle, Snapshots, call, changes,
};

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
    pub pausable: bool,
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
            pausable: fields
                .next()
                .map(bool::try_from)
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
            snapshot.pausable,
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

pub type NightLightProvider = Provider<NightLightProviderState>;
pub type NightLightProviderHandle = ProviderHandle<NightLightProviderState>;

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
            unavailable: Some(reason.into()),
        }
    }
}

impl Follow for NightLightProviderState {
    type Proxy = NightLight1Proxy<'static>;
    type Error = NightLightProviderError;

    const NAME: &'static str = "night light";

    fn lost(reason: String, _: Option<&Self>) -> Self {
        Self::unavailable(reason)
    }

    fn reason(&self) -> Option<&str> {
        self.unavailable.as_deref()
    }
}

impl Snapshots for NightLightProviderState {
    type Snapshot = NightLightSnapshot;

    const BUS_NAME: &'static str = GLIMPSE_NIGHT_LIGHT_BUS_NAME;

    async fn build(
        connection: &zbus::Connection,
        owner: zbus::names::OwnedUniqueName,
    ) -> zbus::Result<Self::Proxy> {
        NightLight1Proxy::builder(connection)
            .destination(owner)?
            .cache_properties(CacheProperties::Yes)
            .build()
            .await
    }

    async fn read(proxy: &Self::Proxy) -> zbus::Result<Self::Snapshot> {
        match proxy.cached_snapshot()? {
            Some(snapshot) => Ok(snapshot),
            None => proxy.snapshot().await,
        }
    }

    async fn changes(proxy: &Self::Proxy) -> Changes<Self::Snapshot> {
        changes(proxy.receive_snapshot_changed().await)
    }

    fn decode(snapshot: Self::Snapshot) -> Result<Self, String> {
        Ok(Self::decoded(snapshot))
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

impl ProviderError for NightLightProviderError {
    fn unavailable(reason: String) -> Self {
        Self::Unavailable(reason)
    }

    fn timed_out() -> Self {
        Self::TimedOut
    }

    fn call(reason: String) -> Self {
        Self::Call(reason)
    }

    fn named(name: &str, reason: String) -> Option<Self> {
        match name {
            "me.aresa.Glimpse.NightLight1.Error.InvalidSchedule" => {
                Some(Self::InvalidSchedule(reason))
            }
            "me.aresa.Glimpse.NightLight1.Error.InvalidTemperature" => {
                Some(Self::InvalidTemperature(reason))
            }
            "me.aresa.Glimpse.NightLight1.Error.NotActive" => Some(Self::NotActive(reason)),
            "me.aresa.Glimpse.NightLight1.Error.Unavailable" => Some(Self::Unavailable(reason)),
            _ => None,
        }
    }
}

impl NightLightProviderHandle {
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

#[cfg(test)]
mod tests;
