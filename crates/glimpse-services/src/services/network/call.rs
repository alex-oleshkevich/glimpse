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

pub async fn activate_vpn(connection: &Connection, saved: &str) -> zbus::Result<()> {
    manager(connection)
        .await?
        .activate_connection(&path(saved)?, &path(ROOT)?, &path(ROOT)?)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secured_network_carries_a_key_management_setting_and_a_secret_only_when_one_was_typed() {
        let settings = wifi_settings(b"Skylink", nm::Security::Wpa2, false, None);
        let guard = settings
            .get("802-11-wireless-security")
            .expect("a security setting");

        assert_eq!(guard.get("key-mgmt"), Some(&Value::from("wpa-psk")));
        assert!(
            !guard.contains_key("psk"),
            "with nothing typed the agent supplies it, and NetworkManager stores what it returns"
        );

        let asked = wifi_settings(
            b"Skylink",
            nm::Security::Wpa2,
            false,
            Some("hunter2hunter2"),
        );
        assert_eq!(
            asked["802-11-wireless-security"].get("psk"),
            Some(&Value::from("hunter2hunter2")),
            "a password typed before the join travels with the profile, so NetworkManager never \
             tears down the working connection for one it then has to ask about"
        );

        let wep = wifi_settings(b"Ancient", nm::Security::Wep, false, Some("s3cr3"));
        assert_eq!(
            wep["802-11-wireless-security"].get("wep-key0"),
            Some(&Value::from("s3cr3")),
            "WEP keeps its key under its own name"
        );
        assert!(!wep["802-11-wireless-security"].contains_key("psk"));
    }

    #[test]
    fn an_open_network_carries_no_security_setting_at_all() {
        let settings = wifi_settings(b"Kaffeehaus", nm::Security::Open, false, None);
        assert!(!settings.contains_key("802-11-wireless-security"));
    }

    #[test]
    fn every_key_management_a_profile_can_carry_is_the_one_the_network_advertises() {
        assert_eq!(key_mgmt(nm::Security::Wpa2), Some("wpa-psk"));
        assert_eq!(key_mgmt(nm::Security::Wpa), Some("wpa-psk"));
        assert_eq!(key_mgmt(nm::Security::Wpa3), Some("sae"));
        assert_eq!(key_mgmt(nm::Security::Wep), Some("none"));
        assert_eq!(
            key_mgmt(nm::Security::Owe),
            Some("owe"),
            "an encrypted open network needs the setting even though it needs no password"
        );
        assert_eq!(key_mgmt(nm::Security::Open), None);
        assert_eq!(
            key_mgmt(nm::Security::Enterprise),
            None,
            "802.1X written as a PSK profile is refused by NetworkManager"
        );
        assert!(!joinable(nm::Security::Enterprise));
        assert!(joinable(nm::Security::Owe));
    }

    #[test]
    fn an_encrypted_open_network_carries_its_key_management_and_no_secret() {
        let settings = wifi_settings(b"Cafe", nm::Security::Owe, false, None);
        let guard = settings
            .get("802-11-wireless-security")
            .expect("owe needs the setting");
        assert_eq!(guard.get("key-mgmt"), Some(&Value::from("owe")));
        assert!(!guard.contains_key("psk"));
    }

    #[test]
    fn wpa3_asks_for_sae_rather_than_psk() {
        let settings = wifi_settings(b"Modern", nm::Security::Wpa3, false, None);
        assert_eq!(
            settings["802-11-wireless-security"].get("key-mgmt"),
            Some(&Value::from("sae"))
        );
    }

    #[test]
    fn a_hidden_network_says_so_in_its_profile() {
        let settings = wifi_settings(b"Skylink Guest", nm::Security::Wpa2, true, None);
        assert_eq!(
            settings["802-11-wireless"].get("hidden"),
            Some(&Value::from(true))
        );

        let visible = wifi_settings(b"Skylink", nm::Security::Wpa2, false, None);
        assert!(
            !visible["802-11-wireless"].contains_key("hidden"),
            "a visible network must not be marked hidden"
        );
    }

    #[test]
    fn an_ssid_goes_on_the_wire_as_bytes_and_not_as_a_string() {
        let raw: &[u8] = &[0x53, 0x6b, 0x79, 0x6c, 0x69, 0x6e, 0x6b];
        let settings = wifi_settings(raw, nm::Security::Wpa2, false, None);
        assert_eq!(
            settings["802-11-wireless"].get("ssid"),
            Some(&Value::from(raw))
        );
    }
}
