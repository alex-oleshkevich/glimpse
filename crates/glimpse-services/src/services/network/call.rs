use std::collections::HashMap;

use zbus::Connection;
use zbus::zvariant::{ObjectPath, Value};

use glimpse_dbus::network_manager as nm;

type Setting<'a> = HashMap<&'a str, Value<'a>>;
type Settings<'a> = HashMap<&'a str, Setting<'a>>;

const ROOT: &str = "/";

fn path(value: &str) -> zbus::Result<ObjectPath<'_>> {
    ObjectPath::try_from(value).map_err(zbus::Error::from)
}

async fn manager(connection: &Connection) -> zbus::Result<nm::NetworkManagerProxy<'static>> {
    nm::NetworkManagerProxy::new(connection).await
}

async fn wireless(
    connection: &Connection,
    device: &str,
) -> zbus::Result<nm::DeviceWirelessProxy<'static>> {
    nm::DeviceWirelessProxy::builder(connection)
        .path(device.to_owned())?
        .build()
        .await
}

async fn profile(
    connection: &Connection,
    saved: &str,
) -> zbus::Result<nm::SettingsConnectionProxy<'static>> {
    nm::SettingsConnectionProxy::builder(connection)
        .path(saved.to_owned())?
        .build()
        .await
}

pub async fn scan(connection: &Connection, device: &str) -> zbus::Result<()> {
    wireless(connection, device)
        .await?
        .request_scan(HashMap::new())
        .await
}

pub async fn activate(
    connection: &Connection,
    saved: &str,
    device: &str,
    specific: &str,
) -> zbus::Result<()> {
    manager(connection)
        .await?
        .activate_connection(&path(saved)?, &path(device)?, &path(specific)?)
        .await
        .map(|_| ())
}

fn wifi_settings<'a>(
    ssid: &'a [u8],
    security: nm::Security,
    hidden: bool,
    secret: Option<&'a str>,
) -> Settings<'a> {
    let mut connection: Setting<'a> = HashMap::new();
    connection.insert("type", Value::from("802-11-wireless"));

    let mut wireless: Setting<'a> = HashMap::new();
    wireless.insert("ssid", Value::from(ssid));
    wireless.insert("mode", Value::from("infrastructure"));
    if hidden {
        wireless.insert("hidden", Value::from(true));
    }

    let mut settings: Settings<'a> = HashMap::new();
    settings.insert("connection", connection);
    settings.insert("802-11-wireless", wireless);

    if let Some(management) = key_mgmt(security) {
        let mut guard: Setting<'a> = HashMap::new();
        guard.insert("key-mgmt", Value::from(management));
        if let Some(secret) = secret {
            guard.insert(
                match security {
                    nm::Security::Wep => "wep-key0",
                    _ => "psk",
                },
                Value::from(secret),
            );
        }
        settings.insert("802-11-wireless-security", guard);
    }
    settings
}

/// The `key-mgmt` a fresh profile carries, or `None` for a network that needs no security setting.
/// **Enterprise is not one of them**: 802.1X wants a certificate, an identity and a phase-2 method,
/// none of which a password box collects, so it is refused rather than written as a PSK profile
/// NetworkManager will reject.
fn key_mgmt(security: nm::Security) -> Option<&'static str> {
    match security {
        nm::Security::Open => None,
        nm::Security::Owe => Some("owe"),
        nm::Security::Wep => Some("none"),
        nm::Security::Wpa3 => Some("sae"),
        nm::Security::Enterprise => None,
        nm::Security::Wpa | nm::Security::Wpa2 => Some("wpa-psk"),
    }
}

pub fn joinable(security: nm::Security) -> bool {
    !security.is_enterprise()
}

/// Whether a saved profile can join this access point. Two profiles may carry the same SSID with
/// different security, and activating the wrong one fails without ever asking for a password.
/// A WPA3 access point in transition mode answers a `wpa-psk` profile, which is why it takes both.
pub fn fits(profile: &nm::Profile, point: &nm::AccessPointProperties) -> bool {
    let saved = profile.key_mgmt.as_deref();
    match point.security() {
        nm::Security::Open => saved.is_none(),
        nm::Security::Owe => saved == Some("owe"),
        nm::Security::Wep => saved == Some("none"),
        nm::Security::Wpa | nm::Security::Wpa2 => saved == Some("wpa-psk"),
        nm::Security::Wpa3 => saved == Some("sae") || saved == Some("wpa-psk"),
        nm::Security::Enterprise => {
            saved == Some("wpa-eap") || saved == Some("wpa-eap-suite-b-192")
        }
    }
}

pub async fn add_and_activate(
    connection: &Connection,
    ssid: &[u8],
    security: nm::Security,
    hidden: bool,
    secret: Option<&str>,
    device: &str,
    specific: &str,
) -> zbus::Result<()> {
    let specific = if hidden { ROOT } else { specific };
    manager(connection)
        .await?
        .add_and_activate_connection2(
            wifi_settings(ssid, security, hidden, secret),
            &path(device)?,
            &path(specific)?,
            HashMap::new(),
        )
        .await
        .map(|_| ())
}

pub async fn set_networking(connection: &Connection, enabled: bool) -> zbus::Result<()> {
    manager(connection).await?.enable(enabled).await
}

pub async fn set_wifi(connection: &Connection, enabled: bool) -> zbus::Result<()> {
    manager(connection)
        .await?
        .set_wireless_enabled(enabled)
        .await
}

pub async fn deactivate(connection: &Connection, active: &str) -> zbus::Result<()> {
    manager(connection)
        .await?
        .deactivate_connection(&path(active)?)
        .await
}

pub async fn forget(connection: &Connection, saved: &str) -> zbus::Result<()> {
    profile(connection, saved).await?.delete().await
}

pub async fn set_autoconnect(
    connection: &Connection,
    saved: &str,
    autoconnect: bool,
) -> zbus::Result<()> {
    let proxy = profile(connection, saved).await?;
    let current = proxy.get_settings().await?;
    let mut settings: Settings<'_> = current
        .iter()
        .map(|(group, entries)| {
            let entries: Setting<'_> = entries
                .iter()
                .map(|(key, value)| (key.as_str(), Value::from(value.clone())))
                .collect();
            (group.as_str(), entries)
        })
        .collect();
    settings
        .entry("connection")
        .or_default()
        .insert("autoconnect", Value::from(autoconnect));
    proxy.update2(settings, 0, HashMap::new()).await.map(|_| ())
}

/// Bring a device up on whichever saved profile NetworkManager judges best, which is what a wired
/// row needs: it is a device, not a profile, and one may not have been written for it yet.
pub async fn activate_device(connection: &Connection, device: &str) -> zbus::Result<()> {
    manager(connection)
        .await?
        .activate_connection(&path(ROOT)?, &path(device)?, &path(ROOT)?)
        .await
        .map(|_| ())
}

pub async fn activate_vpn(connection: &Connection, saved: &str) -> zbus::Result<()> {
    manager(connection)
        .await?
        .activate_connection(&path(saved)?, &path(ROOT)?, &path(ROOT)?)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests;
