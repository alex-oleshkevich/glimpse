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
async fn status_keeps_known_absent_versionless_and_weather_components_visible() {
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
    for name in [
        "me.aresa.GlimpsePanel",
        "me.aresa.GlimpsePanel.AppletHost",
        "me.aresa.GlimpseOrphan.AppletHost",
    ] {
        glimpse_dbus::own_name(&server, name).await.unwrap();
    }
    let _panel = serve(&server, "/me/aresa/GlimpsePanel", "4.5", Ok(()))
        .await
        .unwrap();
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
    let find = |name| rows.iter().find(|row| row.name == name).unwrap();
    let idle = find("me.aresa.Glimpse.Idle");
    assert_eq!(
        (idle.version.as_deref(), idle.ok, idle.message.as_deref()),
        (Some("1.2"), Some(true), Some(""))
    );
    let lock = find("me.aresa.GlimpseLock");
    assert_eq!(
        (lock.version.as_deref(), lock.ok, lock.message.as_deref()),
        (Some("2.3"), Some(false), Some("bad"))
    );
    let version_only = find("me.aresa.GlimpseVersionOnly");
    assert_eq!(version_only.version.as_deref(), Some("3.4"));
    assert_eq!(version_only.ok, None);
    assert_eq!(version_only.message, None);
    let json = serde_json::to_value(version_only).unwrap();
    assert!(json["ok"].is_null());
    assert!(json["message"].is_null());
    assert_eq!(idle.pid, Some(std::process::id()));
    assert_eq!(
        find("me.aresa.GlimpsePanel").version.as_deref(),
        Some("4.5")
    );
    assert!(!rows.iter().any(|row| row.name.ends_with(".AppletHost")));
    assert_eq!(
        find(GLIMPSE_WEATHER_BUS_NAME).version.as_deref(),
        Some("9.9")
    );
    let versionless = find("me.aresa.GlimpseEmpty");
    assert!(versionless.running);
    assert_eq!(versionless.version, None);
    let absent = find(GLIMPSE_SETTINGS_BUS_NAME);
    assert!(!absent.running);
    assert_eq!(absent.pid, None);
    assert_eq!(absent.unit, None);
    assert_eq!(absent.state, None);
    assert_eq!(absent.version, None);
    assert!(serde_json::to_value(&rows).unwrap().is_array());
}

struct Manager;

#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    #[zbus(name = "GetUnitByPID")]
    fn get_unit_by_pid(&self, pid: u32) -> zbus::zvariant::OwnedObjectPath {
        assert_eq!(pid, std::process::id());
        "/org/freedesktop/systemd1/unit/panel".try_into().unwrap()
    }
}

struct Unit;

#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl Unit {
    #[zbus(property)]
    fn id(&self) -> &str {
        "custom-panel.service"
    }

    #[zbus(property)]
    fn active_state(&self) -> &str {
        "active"
    }
}

#[tokio::test]
async fn custom_panels_share_discovery_and_resolve_actual_pid_unit_and_state() {
    let bus = PrivateBus::start();
    let server = bus.connection().await;
    for name in [
        "org.example.CustomPanel",
        "org.example.CustomPanel.AppletHost",
        "org.freedesktop.systemd1",
    ] {
        glimpse_dbus::own_name(&server, name).await.unwrap();
    }
    server
        .object_server()
        .at("/org/freedesktop/systemd1", Manager)
        .await
        .unwrap();
    server
        .object_server()
        .at("/org/freedesktop/systemd1/unit/panel", Unit)
        .await
        .unwrap();
    let client = bus.connection().await;
    let rows = rows(&client).await.unwrap();
    let panel = rows
        .iter()
        .find(|row| row.name == "org.example.CustomPanel")
        .unwrap();
    assert_eq!(panel.pid, Some(std::process::id()));
    assert_eq!(panel.unit.as_deref(), Some("custom-panel.service"));
    assert_eq!(panel.state.as_deref(), Some("active"));
    assert_eq!(panel.version, None);
}
