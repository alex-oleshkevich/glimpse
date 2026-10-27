use std::collections::HashMap;
use std::time::Duration;

use futures_util::stream::BoxStream;
use futures_util::{Stream, StreamExt};

use crate::Delivery;

use chrono::{DateTime, Utc};
use zbus::zvariant::{OwnedValue, Value};

pub const DEADLINE: Duration = Duration::from_secs(5);

pub(crate) type Owners = BoxStream<'static, Delivery<(bool, bool)>>;

pub(crate) async fn owners(
    dbus: &zbus::fdo::DBusProxy<'_>,
    name: &'static str,
) -> zbus::Result<Owners> {
    dbus.receive_name_owner_changed_with_args(&[(0, name)])
        .await
        .map(|stream| {
            crate::events(stream.filter_map(|signal| async move {
                let args = signal.args().ok()?;
                Some((args.old_owner().is_some(), args.new_owner().is_some()))
            }))
        })
}

pub(crate) async fn provider_owner(
    dbus: &zbus::fdo::DBusProxy<'_>,
    owners: &mut Owners,
    name: &'static str,
    initial: bool,
    delay: &mut RetryDelay,
) -> zbus::Result<Option<zbus::names::OwnedUniqueName>> {
    let name = match zbus::names::WellKnownName::try_from(name) {
        Ok(name) => name,
        Err(error) => {
            tracing::warn!(%error, "invalid provider bus name");
            return Ok(None);
        }
    };
    let mut activate = initial;
    let mut waited = initial || !delay.1;
    loop {
        let owned = tokio::time::timeout(DEADLINE, dbus.get_name_owner(name.clone().into())).await;
        match owned {
            Ok(Ok(owner)) if waited => {
                delay.1 = true;
                return Ok(Some(owner));
            }
            Ok(Err(zbus::fdo::Error::NameHasNoOwner(_))) => {
                if activate {
                    activate = false;
                    let activatable = tokio::time::timeout(DEADLINE, dbus.list_activatable_names())
                        .await
                        .is_ok_and(|names| {
                            names.is_ok_and(|names| {
                                names.iter().any(|n| n.as_str() == name.as_str())
                            })
                        });
                    if !activatable {
                        return Err(zbus::Error::Failure(follower::NO_OWNER.to_owned()));
                    }
                    tokio::time::timeout(DEADLINE, dbus.start_service_by_name(name.clone(), 0))
                        .await
                        .map_err(|_| {
                            zbus::Error::Failure("provider activation timed out".to_owned())
                        })??;
                    waited = true;
                    continue;
                }
                if owners.next().await.is_none() {
                    return Ok(None);
                }
            }
            owned => {
                if !matches!(owned, Ok(Ok(_))) {
                    tracing::warn!(?owned, %name, "cannot observe provider ownership");
                }
                if !delay.wait(owners).await {
                    return Ok(None);
                }
            }
        }
        waited = true;
    }
}

pub(crate) struct RetryDelay(Duration, bool);

impl Default for RetryDelay {
    fn default() -> Self {
        Self(Duration::from_millis(250), false)
    }
}

impl RetryDelay {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    async fn wait(&mut self, owners: &mut (impl Stream + Unpin)) -> bool {
        let retry = tokio::select! {
            _ = tokio::time::sleep(self.0) => true,
            owner = owners.next() => owner.is_some(),
        };
        self.0 = (self.0 * 2).min(Duration::from_secs(30));
        retry
    }
}

pub(crate) type Properties = HashMap<String, OwnedValue>;

pub(crate) fn text(properties: &Properties, key: &str, cap: usize) -> Option<String> {
    optional_clean(<&str>::try_from(properties.get(key)?).ok()?.to_owned(), cap)
}

pub(crate) fn flag(properties: &Properties, key: &str) -> Option<bool> {
    bool::try_from(properties.get(key)?).ok()
}

pub(crate) fn number(properties: &Properties, key: &str) -> Option<u32> {
    u32::try_from(properties.get(key)?).ok()
}

pub(crate) fn code(properties: &Properties, key: &str) -> u32 {
    number(properties, key).unwrap_or_default()
}

pub(crate) fn path(properties: &Properties, key: &str) -> Option<String> {
    match &**properties.get(key)? {
        Value::ObjectPath(value) if value.as_str() != "/" => Some(value.as_str().to_owned()),
        _ => None,
    }
}

pub(crate) fn optional_clean(value: String, limit: usize) -> Option<String> {
    let value = glimpse_utils::clean(&value, limit);
    (!value.is_empty()).then_some(value)
}

pub(crate) fn epoch(value: i64) -> Result<DateTime<Utc>, String> {
    DateTime::from_timestamp_micros(value)
        .ok_or_else(|| format!("a snapshot contains invalid timestamp {value}"))
}

pub mod accounts;
pub mod applet_host;
pub mod bluez;
pub mod calendar;
pub mod clipboard;
pub mod dbusmenu;
mod follower;
pub use follower::{
    Changes, Follow, PENDING, Provider, ProviderError, ProviderHandle, Snapshots, call, changes,
};
pub mod freedesktop_notifications;
pub mod geoclue;
pub mod gtk_actions;
pub mod idle;
pub mod kdeconnect;
pub mod login1;
pub mod mpris;
pub mod network_manager;
pub mod night_light;
pub mod notifications;
pub mod obex;
pub mod polkit;
pub mod power_profiles;
pub mod session;
pub mod settings;
pub mod status_notifier_item;
pub mod status_notifier_watcher;
pub mod systemd1;
pub mod udisks2;
pub mod upower;
pub mod weather;

#[cfg(test)]
mod tests;
