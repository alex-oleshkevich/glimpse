use std::future::Future;

use chrono::{DateTime, Utc};
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio_util::task::AbortOnDropHandle;
use zbus::proxy::CacheProperties;

use super::weather::GeoCoordinates;

pub const GLIMPSE_SETTINGS_BUS_NAME: &str = "me.aresa.Glimpse.Settings";
pub const GLIMPSE_SETTINGS_OBJECT_PATH: &str = "/me/aresa/Glimpse/Settings";

pub type SolarSnapshot = (bool, u32, i64, String);
pub type GeolocationSnapshot = (bool, f64, f64, String);
pub type ModeSnapshot = (bool, bool, u32, u32, u32, String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Light,
    Dark,
}

impl Mode {
    pub fn code(self) -> u32 {
        match self {
            Self::Light => 1,
            Self::Dark => 2,
        }
    }

    pub fn decode(code: u32) -> Result<Option<Self>, String> {
        match code {
            0 => Ok(None),
            1 => Ok(Some(Self::Light)),
            2 => Ok(Some(Self::Dark)),
            _ => Err(format!("invalid desktop mode {code}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModeStatus {
    pub automatic: bool,
    pub requested: Option<Mode>,
    pub observed: Option<Mode>,
    pub temporary: Option<Mode>,
}

pub fn decode_mode(snapshot: ModeSnapshot) -> Result<ModeStatus, String> {
    let (available, automatic, requested, observed, temporary, reason) = snapshot;
    if available && !reason.is_empty() {
        return Err("an available desktop mode carries an error".to_owned());
    }
    Ok(ModeStatus {
        automatic,
        requested: Mode::decode(requested)?,
        observed: Mode::decode(observed)?,
        temporary: Mode::decode(temporary)?,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SolarPhase {
    Day,
    Night,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolarStatus {
    pub phase: SolarPhase,
    #[serde(default)]
    pub next_change: Option<DateTime<Utc>>,
}

#[zbus::proxy(
    interface = "me.aresa.Glimpse.Settings1",
    default_service = "me.aresa.Glimpse.Settings",
    default_path = "/me/aresa/Glimpse/Settings"
)]
pub trait Settings1 {
    #[zbus(property)]
    fn solar(&self) -> zbus::Result<SolarSnapshot>;

    #[zbus(property)]
    fn geolocation(&self) -> zbus::Result<GeolocationSnapshot>;

    #[zbus(property)]
    fn mode(&self) -> zbus::Result<ModeSnapshot>;

    fn set_temporary_mode(&self, mode: u32) -> zbus::Result<()>;

    fn resume(&self) -> zbus::Result<()>;
}

pub struct SettingsProvider {
    state: watch::Receiver<Option<SolarStatus>>,
    location: watch::Receiver<Option<GeoCoordinates>>,
    task: AbortOnDropHandle<()>,
}

impl SettingsProvider {
    pub fn start(connection: zbus::Connection) -> Self {
        let (updates, state) = watch::channel(None);
        let (locations, location) = watch::channel(None);
        let task = AbortOnDropHandle::new(tokio::spawn(follow(connection, updates, locations)));
        Self {
            state,
            location,
            task,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<SolarStatus>> {
        self.state.clone()
    }

    pub fn subscribe_location(&self) -> watch::Receiver<Option<GeoCoordinates>> {
        self.location.clone()
    }

    pub async fn shutdown(self) {
        self.task.abort();
        let _ = self.task.await;
    }
}

pub fn decode_geolocation(snapshot: GeolocationSnapshot) -> Result<Option<GeoCoordinates>, String> {
    let (has_fix, latitude, longitude, _) = snapshot;
    if !has_fix {
        return Ok(None);
    }
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return Err("geolocation coordinates are out of range".to_owned());
    }
    Ok(Some(GeoCoordinates {
        latitude,
        longitude,
    }))
}

pub fn decode_solar(snapshot: SolarSnapshot) -> Result<Option<SolarStatus>, String> {
    let (available, phase, next_change, reason) = snapshot;
    if !available {
        return Ok(None);
    }
    let phase = match phase {
        1 => SolarPhase::Day,
        2 => SolarPhase::Night,
        _ => return Err(format!("invalid solar phase {phase}")),
    };
    if !reason.is_empty() {
        return Err("an available solar phase carries an error".to_owned());
    }
    let next_change = match next_change {
        0 => None,
        timestamp => Some(super::epoch(timestamp)?),
    };
    Ok(Some(SolarStatus { phase, next_change }))
}

async fn bounded<T>(request: impl Future<Output = zbus::Result<T>>) -> zbus::Result<T> {
    tokio::time::timeout(super::DEADLINE, request)
        .await
        .map_err(|_| zbus::Error::Failure("settings request timed out".to_owned()))?
}

fn publish<T: PartialEq>(updates: &watch::Sender<Option<T>>, snapshot: Result<Option<T>, String>) {
    let next = snapshot.unwrap_or_else(|reason| {
        tracing::debug!(
            reason = glimpse_utils::clean(&reason, 240),
            "settings state unavailable"
        );
        None
    });
    updates.send_if_modified(|current| {
        if *current == next {
            false
        } else {
            *current = next;
            true
        }
    });
}

async fn follow(
    connection: zbus::Connection,
    updates: watch::Sender<Option<SolarStatus>>,
    locations: watch::Sender<Option<GeoCoordinates>>,
) {
    let dbus = match zbus::fdo::DBusProxy::new(&connection).await {
        Ok(dbus) => dbus,
        Err(error) => {
            tracing::warn!(%error, "cannot observe the settings provider");
            return;
        }
    };
    let mut owners = match dbus
        .receive_name_owner_changed_with_args(&[(0, GLIMPSE_SETTINGS_BUS_NAME)])
        .await
    {
        Ok(owners) => owners,
        Err(error) => {
            tracing::warn!(%error, "cannot observe settings owner changes");
            return;
        }
    };
    let name = match zbus::names::BusName::try_from(GLIMPSE_SETTINGS_BUS_NAME) {
        Ok(name) => name,
        Err(error) => {
            tracing::warn!(%error, "invalid provider bus name");
            return;
        }
    };
    let mut retry_delay = std::time::Duration::from_millis(250);

    loop {
        let proxy = bounded(
            Settings1Proxy::builder(&connection)
                .cache_properties(CacheProperties::Yes)
                .build(),
        )
        .await;
        let proxy = match proxy {
            Ok(proxy) => {
                retry_delay = std::time::Duration::from_millis(250);
                proxy
            }
            Err(error) => {
                publish(&updates, Err(error.to_string()));
                publish(&locations, Ok(None));
                if !super::retry_after_failure(&dbus, &mut owners, &name, &mut retry_delay).await {
                    return;
                }
                continue;
            }
        };
        let mut changes = proxy.receive_solar_changed().await;
        let mut location_changes = proxy.receive_geolocation_changed().await;
        publish(
            &updates,
            bounded(proxy.solar())
                .await
                .map_err(|e| e.to_string())
                .and_then(decode_solar),
        );
        publish(
            &locations,
            bounded(proxy.geolocation())
                .await
                .map_err(|e| e.to_string())
                .and_then(decode_geolocation),
        );
        loop {
            tokio::select! {
                changed = changes.next() => {
                    let Some(changed) = changed else { break };
                    publish(&updates, bounded(changed.get()).await.map_err(|e| e.to_string()).and_then(decode_solar));
                }
                changed = location_changes.next() => {
                    let Some(changed) = changed else { break };
                    publish(&locations, bounded(changed.get()).await.map_err(|e| e.to_string()).and_then(decode_geolocation));
                }
                owner = owners.next() => {
                    let Some(owner) = owner else {
                        publish(&updates, Err("settings owner stream ended".to_owned()));
                        publish(&locations, Ok(None));
                        return;
                    };
                    if let Ok(args) = owner.args()
                        && (args.old_owner().is_some()
                            || (args.new_owner().is_some() && updates.borrow().is_none()))
                    {
                        break;
                    }
                }
            }
        }
        publish(&updates, Err("settings owner changed".to_owned()));
        publish(&locations, Ok(None));
    }
}

#[cfg(test)]
mod tests;
