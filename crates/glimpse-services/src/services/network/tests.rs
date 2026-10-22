use super::*;

use zbus::zvariant::Value;

fn owned<'a, T: Into<Value<'a>>>(value: T) -> OwnedValue {
    OwnedValue::try_from(value.into()).expect("a representable value")
}

fn properties(pairs: Vec<(&str, OwnedValue)>) -> Properties {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

fn device(kind: u32, managed: bool) -> nm::DeviceProperties {
    nm::decode_device(&properties(vec![
        ("DeviceType", owned(kind)),
        ("Managed", owned(managed)),
        ("State", owned(100u32)),
    ]))
}

fn access(ssid: &str, strength: u8, flags: u32, active: bool) -> Access {
    Access {
        id: NetworkId::new(format!("/ap/{ssid}/{strength}")),
        ssid: (!ssid.is_empty()).then(|| ssid.to_owned()),
        bssid: Some(format!("00:00:00:00:00:{strength:02x}")),
        strength,
        band: nm::Band::TwoPointFour,
        security: nm::Security::read(flags, 0, 392),
        active,
        saved: None,
        address: None,
        busy: None,
        failure: None,
    }
}

fn wifi_access(ssid: &str, strength: u8, active: bool) -> Access {
    Access {
        id: NetworkId::new("/ap/1"),
        ssid: (!ssid.is_empty()).then(|| ssid.to_owned()),
        bssid: None,
        strength,
        band: nm::Band::Five,
        security: nm::Security::Wpa2,
        active,
        saved: None,
        address: None,
        busy: None,
        failure: None,
    }
}

fn wifi_connected(strength: u8) -> NetworkState {
    NetworkState {
        networking: true,
        wifi: Some(Radio {
            enabled: true,
            hardware_enabled: true,
        }),
        connectivity: nm::Connectivity::Full,
        networks: vec![wifi_access("Skylink", strength, true)],
        ..NetworkState::default()
    }
}

mod icon;
mod model;
mod profiles;
mod review;
mod scanning;
pub(super) mod service;
