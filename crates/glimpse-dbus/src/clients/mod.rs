use chrono::{DateTime, Utc};

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
pub mod glimpse_lock;
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
pub mod systemd1;
pub mod timedate1;
pub mod udisks2;
pub mod upower;
pub mod weather;
