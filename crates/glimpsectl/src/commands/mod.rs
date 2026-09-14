mod config;
mod doctor;
mod notifications;
mod sunset;
mod weather;

pub use config::{config_path, config_show, config_validate};
pub use doctor::doctor;
pub use notifications::{
    notifications_clear, notifications_dismiss, notifications_dnd, notifications_list,
};
pub use sunset::{sunset_mode, sunset_status};
pub use weather::{weather_refresh, weather_status};

use anyhow::Result;
use serde::Serialize;
use zbus::Connection;
use zbus::proxy::CacheProperties;

use crate::render;

const ABSENT: &str = "-";

pub(super) async fn proxy<'a, P>(connection: &'a Connection) -> zbus::Result<P>
where
    P: From<zbus::Proxy<'a>> + zbus::proxy::Defaults,
{
    zbus::proxy::Builder::<P>::new(connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

pub(super) async fn within<T, E>(request: impl Future<Output = Result<T, E>>) -> Result<T>
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

pub(super) fn safe(text: &str) -> String {
    glimpse_utils::clean(text, REASON)
}

const REASON: usize = 240;

pub(super) fn yes_no(value: bool) -> &'static str {
    match value {
        true => "yes",
        false => "no",
    }
}

pub(super) fn reason_or_absent(reason: &str) -> String {
    match reason.is_empty() {
        true => ABSENT.to_owned(),
        false => reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_healthy_provider_has_no_reason_to_show() {
        assert_eq!(reason_or_absent(""), ABSENT);
        assert_eq!(reason_or_absent("no location fix"), "no location fix");
    }

    #[test]
    fn a_flag_reads_as_a_word() {
        assert_eq!(yes_no(true), "yes");
        assert_eq!(yes_no(false), "no");
    }
}
