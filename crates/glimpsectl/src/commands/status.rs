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
        &Table::new()
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
mod tests {
    use super::*;
    use glimpse_dbus::actions::serve;
    use glimpse_dbus::testing::PrivateBus;
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedValue, Signature, Str};

    struct VersionOnly;

    #[zbus::interface(name = "org.gtk.Actions")]
    impl VersionOnly {
        fn describe_all(&self) -> HashMap<String, (bool, Signature, Vec<OwnedValue>)> {
            HashMap::from([(
                "version".into(),
                (
                    true,
                    Signature::Unit,
                    vec![OwnedValue::from(Str::from("3.4"))],
                ),
            )])
        }
    }

    #[tokio::test]
    async fn status_reads_only_served_components() {
        let bus = PrivateBus::start();
        let server = bus.connection().await;
        glimpse_dbus::own_name(&server, "me.aresa.Glimpse.Idle")
            .await
            .expect("the name");
        glimpse_dbus::own_name(&server, "me.aresa.GlimpseLock")
            .await
            .expect("the name");
        glimpse_dbus::own_name(&server, "me.aresa.GlimpseEmpty")
            .await
            .expect("the name");
        glimpse_dbus::own_name(&server, "me.aresa.Glimpse.Weather")
            .await
            .expect("the name");
        glimpse_dbus::own_name(&server, "me.aresa.GlimpseVersionOnly")
            .await
            .expect("the name");
        let _idle = serve(&server, "/me/aresa/Glimpse/Idle", "1.2", Ok(()))
            .await
            .unwrap();
        let _lock = serve(&server, "/me/aresa/GlimpseLock", "2.3", Err("bad".into()))
            .await
            .unwrap();
        let _weather = serve(&server, "/me/aresa/Glimpse/Weather", "9.9", Ok(()))
            .await
            .unwrap();
        server
            .object_server()
            .at("/me/aresa/GlimpseVersionOnly", VersionOnly)
            .await
            .unwrap();
        let client = bus.connection().await;
        let rows = rows(&client).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "me.aresa.Glimpse.Idle");
        assert_eq!(
            (
                rows[0].version.as_str(),
                rows[0].ok,
                rows[0].message.as_deref()
            ),
            ("1.2", Some(true), Some(""))
        );
        assert_eq!(rows[1].name, "me.aresa.GlimpseLock");
        assert_eq!(
            (
                rows[1].version.as_str(),
                rows[1].ok,
                rows[1].message.as_deref()
            ),
            ("2.3", Some(false), Some("bad"))
        );
        assert_eq!(rows[2].name, "me.aresa.GlimpseVersionOnly");
        assert_eq!(rows[2].version, "3.4");
        assert_eq!(rows[2].ok, None);
        assert_eq!(rows[2].message, None);
        let json = serde_json::to_value(&rows[2]).unwrap();
        assert!(json["ok"].is_null());
        assert!(json["message"].is_null());
    }
}
