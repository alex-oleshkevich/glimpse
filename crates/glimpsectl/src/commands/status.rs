use anyhow::Result;
use glimpse_dbus::gtk_actions::{GtkActionsProxy, object_path};
use glimpse_dbus::{
    clipboard::GLIMPSE_CLIPBOARD_BUS_NAME,
    idle::GLIMPSE_IDLE_BUS_NAME,
    night_light::GLIMPSE_NIGHT_LIGHT_BUS_NAME,
    notifications::GLIMPSE_NOTIFICATIONS_BUS_NAME,
    settings::GLIMPSE_SETTINGS_BUS_NAME,
    systemd1::{Systemd1ManagerProxy, Systemd1UnitProxy},
    weather::GLIMPSE_WEATHER_BUS_NAME,
};
use serde::Serialize;
use std::collections::BTreeSet;
use zbus::Connection;
use zbus::proxy::CacheProperties;
use zbus::zvariant::Structure;

use super::{emit, safe, within};
use crate::render::Table;

const KNOWN_COMPONENTS: &[&str] = &[
    GLIMPSE_IDLE_BUS_NAME,
    GLIMPSE_NIGHT_LIGHT_BUS_NAME,
    GLIMPSE_NOTIFICATIONS_BUS_NAME,
    GLIMPSE_SETTINGS_BUS_NAME,
    GLIMPSE_WEATHER_BUS_NAME,
    GLIMPSE_CLIPBOARD_BUS_NAME,
    "me.aresa.GlimpsePanel",
    "me.aresa.GlimpseNotifications",
    "me.aresa.GlimpseLock",
    "me.aresa.GlimpseWallpaper",
    "me.aresa.Glimpse.Shot",
    "me.aresa.Glimpse.Shot.Portal",
    "me.aresa.Glimpse.Idle.Portal",
];

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(super) struct ComponentRow {
    name: String,
    version: Option<String>,
    ok: Option<bool>,
    message: Option<String>,
    running: bool,
    pid: Option<u32>,
    unit: Option<String>,
    state: Option<String>,
}

pub(super) fn absent_rows() -> Vec<ComponentRow> {
    KNOWN_COMPONENTS
        .iter()
        .map(|name| ComponentRow {
            name: (*name).to_owned(),
            version: None,
            ok: None,
            message: None,
            running: false,
            pid: None,
            unit: None,
            state: None,
        })
        .collect()
}

pub(super) async fn rows(connection: &Connection) -> Result<Vec<ComponentRow>> {
    let dbus = within(zbus::fdo::DBusProxy::new(connection)).await?;
    let owned = within(dbus.list_names()).await?;
    let mut names = KNOWN_COMPONENTS
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<BTreeSet<_>>();
    names.extend(
        owned
            .iter()
            .map(|name| name.as_str())
            .filter(|name| name.starts_with("me.aresa.Glimpse"))
            .map(str::to_owned),
    );
    names.extend(super::panel::panel_names(connection).await?);
    let manager = match within(dbus.get_name_owner("org.freedesktop.systemd1".try_into()?)).await {
        Ok(owner) => within(
            Systemd1ManagerProxy::builder(connection)
                .destination(owner)?
                .cache_properties(CacheProperties::No)
                .build(),
        )
        .await
        .ok(),
        Err(_) => None,
    };
    let mut found = Vec::new();
    for name in names {
        let owner = within(dbus.get_name_owner(name.as_str().try_into()?))
            .await
            .ok();
        let running = owner.is_some();
        let pid = match &owner {
            Some(owner) => within(dbus.get_connection_unix_process_id(owner.as_str().try_into()?))
                .await
                .ok(),
            None => None,
        };
        let (unit, state) = async {
            let manager = manager.as_ref()?;
            let path = within(manager.get_unit_by_pid(pid?)).await.ok()?;
            let unit = within(
                Systemd1UnitProxy::builder(connection)
                    .destination(manager.inner().destination().clone())
                    .ok()?
                    .path(path)
                    .ok()?
                    .cache_properties(CacheProperties::No)
                    .build(),
            )
            .await
            .ok()?;
            let (id, state) = tokio::join!(within(unit.id()), within(unit.active_state()));
            Some((
                id.ok().map(|value| safe(&value)),
                state.ok().map(|value| safe(&value)),
            ))
        }
        .await
        .unwrap_or((None, None));
        let path = object_path(&name);
        let descriptions = match owner {
            Some(owner) => match within(
                GtkActionsProxy::builder(connection)
                    .destination(owner)?
                    .path(path.as_str())?
                    .cache_properties(CacheProperties::No)
                    .build(),
            )
            .await
            {
                Ok(actions) => within(actions.describe_all()).await.ok(),
                Err(_) => None,
            },
            None => None,
        };
        let version = descriptions
            .as_ref()
            .and_then(|descriptions| descriptions.get("version"))
            .and_then(|(_, _, state)| state.first())
            .and_then(|value| String::try_from(value.clone()).ok())
            .map(|version| safe(&version));
        let status = descriptions
            .as_ref()
            .and_then(|descriptions| descriptions.get("config-status"))
            .and_then(|(_, _, state)| state.first())
            .and_then(|value| Structure::try_from(value.clone()).ok())
            .and_then(|value| <(bool, String)>::try_from(value).ok());
        found.push(ComponentRow {
            name: safe(&name),
            version,
            ok: status.as_ref().map(|(ok, _)| *ok),
            message: status.map(|(_, message)| safe(&message)),
            running,
            pid,
            unit,
            state,
        });
    }
    found.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(found)
}

pub(super) fn print(rows: &[ComponentRow], json: bool) -> Result<()> {
    if json {
        return emit(&rows);
    }
    crate::render::print(
        &Table::default()
            .with_headers(["COMPONENT", "STATE", "PID", "UNIT", "VERSION", "CONFIG"])
            .with_empty("no components")
            .with_rows(rows.iter().map(|row| {
                [
                    row.name.clone(),
                    row.state.clone().unwrap_or_else(|| {
                        if row.running {
                            "running".into()
                        } else {
                            "not running".into()
                        }
                    }),
                    row.pid.map_or_else(|| "-".into(), |pid| pid.to_string()),
                    row.unit.clone().unwrap_or_else(|| "unknown".into()),
                    row.version.clone().unwrap_or_else(|| "unknown".into()),
                    match row.ok {
                        Some(true) => "ok".to_owned(),
                        Some(false) => {
                            format!("rejected: {}", row.message.as_deref().unwrap_or(""))
                        }
                        None => "unknown".to_owned(),
                    },
                ]
            }))
            .render(),
    )?;
    Ok(())
}

pub async fn status(connection: &Connection, json: bool) -> Result<()> {
    print(&rows(connection).await?, json)
}

#[cfg(test)]
mod tests;
