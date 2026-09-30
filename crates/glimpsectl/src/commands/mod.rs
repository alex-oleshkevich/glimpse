pub mod applets;
pub mod applets_author;
pub mod clipboard;
pub mod config;
pub mod doctor;
pub mod idle;
pub mod notifications;
pub mod panel;
pub mod run;
pub mod session;
pub mod status;
pub mod sunset;
pub mod weather;

pub use applets::{applets_inspect, applets_list, applets_restart};
pub use applets_author::applets_new;
pub use clipboard::{
    clipboard_clear, clipboard_copy, clipboard_forget, clipboard_get, clipboard_list, clipboard_pin,
};
pub use config::{config_path, config_show, config_validate};
pub use doctor::doctor;
pub use idle::{idle_inhibit, idle_release, idle_status};
pub use notifications::{
    notifications_clear, notifications_dismiss, notifications_dnd, notifications_list,
};
pub use panel::panel;
pub use run::applets_run;
pub use session::session_declare;
pub use status::status;
pub use sunset::{sunset_mode, sunset_status, sunset_temperature};
pub use weather::{weather_refresh, weather_status};

use anyhow::Result;
use serde::Serialize;
use zbus::Connection;
use zbus::proxy::CacheProperties;

use crate::render;

pub const ABSENT: &str = "-";

pub(super) async fn proxy<'a, P>(connection: &'a Connection) -> zbus::Result<P>
where
    P: From<zbus::Proxy<'a>> + zbus::proxy::Defaults,
{
    zbus::proxy::Builder::<P>::new(connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

pub async fn within<T, E>(request: impl Future<Output = Result<T, E>>) -> Result<T>
where
    E: std::error::Error + Send + Sync + 'static,
{
    match tokio::time::timeout(crate::errors::DEADLINE, request).await {
        Ok(answered) => Ok(answered?),
        Err(_) => Err(crate::errors::TimedOut.into()),
    }
}

pub(super) async fn has_owner(connection: &Connection, name: &str) -> Result<bool> {
    let dbus = zbus::fdo::DBusProxy::new(connection).await?;
    within(dbus.name_has_owner(name.try_into()?)).await
}

pub(super) fn emit<T: Serialize>(value: &T) -> Result<()> {
    render::print(&serde_json::to_string_pretty(value)?)?;
    Ok(())
}

pub fn safe(text: &str) -> String {
    glimpse_utils::clean(text, REASON)
}

const REASON: usize = 240;

pub fn yes_no(value: bool) -> &'static str {
    match value {
        true => "yes",
        false => "no",
    }
}

pub fn reason_or_absent(reason: &str) -> String {
    match reason.is_empty() {
        true => ABSENT.to_owned(),
        false => reason.to_owned(),
    }
}
