use std::collections::HashMap;

use chrono::{DateTime, Utc};
use zbus::zvariant::OwnedValue;

pub const DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

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

pub(crate) fn optional_clean(value: String, limit: usize) -> Option<String> {
    let value = glimpse_utils::clean(&value, limit);
    (!value.is_empty()).then_some(value)
}

pub(crate) fn epoch(value: i64) -> Result<DateTime<Utc>, String> {
    DateTime::from_timestamp_micros(value)
        .ok_or_else(|| format!("a snapshot contains invalid timestamp {value}"))
}

pub mod accounts;
pub mod bluez;
pub mod dbusmenu;
pub mod geoclue;
pub mod hostname1;
pub mod login1;
pub mod mpris;
pub mod network_manager;
pub mod night_light;
pub mod notifications;
pub mod portal_settings;
pub mod power_profiles;
pub mod sensor;
pub mod status_notifier_item;
pub mod status_notifier_watcher;
pub mod systemd1;
pub mod timedate1;
pub mod udisks2;
pub mod upower;
pub mod weather;
