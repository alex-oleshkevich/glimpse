use anyhow::Result;
use glimpse_dbus::gtk_actions::{GtkActionsProxy, object_path};
use serde::Serialize;
use zbus::Connection;
use zbus::proxy::CacheProperties;
use zbus::zvariant::Structure;

use super::{emit, safe, within};
use crate::render::Table;

#[derive(Debug, Serialize, PartialEq, Eq)]
struct ComponentRow {
    name: String,
    version: String,
    ok: Option<bool>,
    message: Option<String>,
}

async fn rows(connection: &Connection) -> Result<Vec<ComponentRow>> {
    let dbus = within(zbus::fdo::DBusProxy::new(connection)).await?;
    let mut found = Vec::new();
    for name in within(dbus.list_names()).await? {
        let name = name.as_str();
        if !name.starts_with("me.aresa.Glimpse")
            || name.starts_with(':')
            || name == "me.aresa.Glimpse.Weather"
        {
            continue;
        }
        let path = object_path(name);
        let Ok(actions) = within(
            GtkActionsProxy::builder(connection)
                .destination(name)?
                .path(path.as_str())?
                .cache_properties(CacheProperties::No)
                .build(),
        )
        .await
        else {
            continue;
        };
        let Ok(descriptions) = within(actions.describe_all()).await else {
            continue;
        };
        let Some(version) = descriptions
            .get("version")
            .and_then(|(_, _, state)| state.first())
            .and_then(|value| String::try_from(value.clone()).ok())
        else {
            continue;
        };
        let status = descriptions
            .get("config-status")
            .and_then(|(_, _, state)| state.first())
            .and_then(|value| Structure::try_from(value.clone()).ok())
            .and_then(|value| <(bool, String)>::try_from(value).ok());
        found.push(ComponentRow {
            name: safe(name),
            version: safe(&version),
            ok: status.as_ref().map(|(ok, _)| *ok),
            message: status.map(|(_, message)| safe(&message)),
        });
    }
    found.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(found)
}

fn print(rows: &[ComponentRow], json: bool) -> Result<()> {
    if json {
        return emit(&rows);
    }
    crate::render::print(
        &Table::default()
            .with_headers(["COMPONENT", "VERSION", "CONFIG"])
            .with_empty("no components")
            .with_rows(rows.iter().map(|row| {
                [
                    row.name.clone(),
                    row.version.clone(),
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
