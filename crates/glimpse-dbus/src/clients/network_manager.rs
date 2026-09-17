use std::collections::HashMap;

use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use super::{Properties, flag, number, optional_clean};

pub const SERVICE: &str = "org.freedesktop.NetworkManager";
pub const MANAGER: &str = "/org/freedesktop/NetworkManager";
pub const SETTINGS: &str = "/org/freedesktop/NetworkManager/Settings";
pub const AGENT_MANAGER: &str = "/org/freedesktop/NetworkManager/AgentManager";

pub const OBJECTS: &str = "/org/freedesktop";

pub const MANAGER1: &str = "org.freedesktop.NetworkManager";
pub const DEVICE1: &str = "org.freedesktop.NetworkManager.Device";
pub const WIRELESS1: &str = "org.freedesktop.NetworkManager.Device.Wireless";
pub const WIRED1: &str = "org.freedesktop.NetworkManager.Device.Wired";
pub const P2P1: &str = "org.freedesktop.NetworkManager.Device.WifiP2P";
pub const ACCESS_POINT1: &str = "org.freedesktop.NetworkManager.AccessPoint";
pub const ACTIVE1: &str = "org.freedesktop.NetworkManager.Connection.Active";
pub const VPN1: &str = "org.freedesktop.NetworkManager.VPN.Connection";
pub const IP4CONFIG1: &str = "org.freedesktop.NetworkManager.IP4Config";
pub const SETTINGS_CONNECTION1: &str = "org.freedesktop.NetworkManager.Settings.Connection";

const SSID: usize = 64;
const SSID_OCTETS: usize = 32;
const ID: usize = 64;
const INTERFACE: usize = 32;

pub const SEC_PAIR_WEP40: u32 = 0x1;
pub const SEC_PAIR_WEP104: u32 = 0x2;
pub const SEC_PAIR_TKIP: u32 = 0x4;
pub const SEC_PAIR_CCMP: u32 = 0x8;
pub const SEC_GROUP_WEP40: u32 = 0x10;
pub const SEC_GROUP_WEP104: u32 = 0x20;
pub const SEC_GROUP_TKIP: u32 = 0x40;
pub const SEC_GROUP_CCMP: u32 = 0x80;
pub const SEC_KEY_MGMT_PSK: u32 = 0x100;
pub const SEC_KEY_MGMT_802_1X: u32 = 0x200;
pub const SEC_KEY_MGMT_SAE: u32 = 0x400;
pub const SEC_KEY_MGMT_OWE: u32 = 0x800;
pub const SEC_KEY_MGMT_OWE_TM: u32 = 0x1000;
pub const SEC_KEY_MGMT_EAP_SUITE_B_192: u32 = 0x2000;

pub const AP_FLAG_PRIVACY: u32 = 0x1;

pub const AGENT_CAPABILITY_VPN_HINTS: u32 = 0x1;

pub const SECRET_FLAG_ALLOW_INTERACTION: u32 = 0x1;
pub const SECRET_FLAG_REQUEST_NEW: u32 = 0x2;
pub const SECRET_FLAG_USER_REQUESTED: u32 = 0x4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeviceKind {
    Ethernet,
    Wifi,
    Bluetooth,
    Modem,
    Bridge,
    Veth,
    WifiP2p,
    Loopback,
    #[default]
    Other,
}

impl DeviceKind {
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::Ethernet,
            2 => Self::Wifi,
            5 => Self::Bluetooth,
            8 => Self::Modem,
            13 => Self::Bridge,
            20 => Self::Veth,
            30 => Self::WifiP2p,
            32 => Self::Loopback,
            _ => Self::Other,
        }
    }

    pub fn user_facing(self) -> bool {
        matches!(self, Self::Ethernet | Self::Wifi | Self::Modem)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeviceState {
    #[default]
    Unknown,
    Unmanaged,
    Unavailable,
    Disconnected,
    Preparing,
    NeedAuth,
    Activated,
    Deactivating,
    Failed,
}

impl DeviceState {
    pub fn from_code(code: u32) -> Self {
        match code {
            10 => Self::Unmanaged,
            20 => Self::Unavailable,
            30 => Self::Disconnected,
            40..=90 if code == 60 => Self::NeedAuth,
            40..=90 => Self::Preparing,
            100 => Self::Activated,
            110 => Self::Deactivating,
            120 => Self::Failed,
            _ => Self::Unknown,
        }
    }

    pub fn busy(self) -> bool {
        matches!(self, Self::Preparing | Self::NeedAuth | Self::Deactivating)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ActiveState {
    #[default]
    Unknown,
    Activating,
    Activated,
    Deactivating,
    Deactivated,
}

impl ActiveState {
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::Activating,
            2 => Self::Activated,
            3 => Self::Deactivating,
            4 => Self::Deactivated,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VpnState {
    #[default]
    Unknown,
    Preparing,
    NeedAuth,
    Connecting,
    GettingAddress,
    Activated,
    Failed,
    Disconnected,
}

impl VpnState {
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::Preparing,
            2 => Self::NeedAuth,
            3 => Self::Connecting,
            4 => Self::GettingAddress,
            5 => Self::Activated,
            6 => Self::Failed,
            7 => Self::Disconnected,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Connectivity {
    #[default]
    Unknown,
    None,
    Portal,
    Limited,
    Full,
}

impl Connectivity {
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::None,
            2 => Self::Portal,
            3 => Self::Limited,
            4 => Self::Full,
            _ => Self::Unknown,
        }
    }

    pub fn reaches_the_internet(self) -> bool {
        matches!(self, Self::Full | Self::Unknown)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Metered {
    #[default]
    Unknown,
    Yes,
    No,
    GuessYes,
    GuessNo,
}

impl Metered {
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::Yes,
            2 => Self::No,
            3 => Self::GuessYes,
            4 => Self::GuessNo,
            _ => Self::Unknown,
        }
    }

    pub fn marked(self) -> bool {
        matches!(self, Self::Yes | Self::GuessYes)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Security {
    #[default]
    Open,
    Wep,
    Wpa,
    Wpa2,
    Wpa3,
    Enterprise,
    Owe,
}

impl Security {
    pub fn read(flags: u32, wpa: u32, rsn: u32) -> Self {
        let both = wpa | rsn;
        if both & (SEC_KEY_MGMT_802_1X | SEC_KEY_MGMT_EAP_SUITE_B_192) != 0 {
            return Self::Enterprise;
        }
        if both & SEC_KEY_MGMT_SAE != 0 {
            return Self::Wpa3;
        }
        if both & (SEC_KEY_MGMT_OWE | SEC_KEY_MGMT_OWE_TM) != 0 {
            return Self::Owe;
        }
        if rsn & SEC_KEY_MGMT_PSK != 0 {
            return Self::Wpa2;
        }
        if wpa & SEC_KEY_MGMT_PSK != 0 {
            return Self::Wpa;
        }
        if flags & AP_FLAG_PRIVACY != 0 {
            return Self::Wep;
        }
        Self::Open
    }

    pub fn needs_a_secret(self) -> bool {
        !matches!(self, Self::Open | Self::Owe)
    }

    pub fn is_enterprise(self) -> bool {
        matches!(self, Self::Enterprise)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Band {
    #[default]
    Unknown,
    TwoPointFour,
    Five,
    Six,
}

impl Band {
    pub fn from_frequency(mhz: u32) -> Self {
        match mhz {
            0 => Self::Unknown,
            1..=2500 => Self::TwoPointFour,
            2501..=5925 => Self::Five,
            _ => Self::Six,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strength {
    #[default]
    None,
    Weak,
    Ok,
    Good,
    Excellent,
}

impl Strength {
    pub fn band(percent: u8) -> Self {
        match percent {
            80..=u8::MAX => Self::Excellent,
            55..=79 => Self::Good,
            30..=54 => Self::Ok,
            5..=29 => Self::Weak,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManagerProperties {
    pub networking_enabled: Option<bool>,
    pub wireless_enabled: Option<bool>,
    pub wireless_hardware_enabled: Option<bool>,
    pub connectivity: Connectivity,
    pub metered: Metered,
    pub primary: Option<String>,
    pub devices: Vec<String>,
    pub active: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceProperties {
    pub kind: DeviceKind,
    pub state: DeviceState,
    pub reason: u32,
    pub interface: Option<String>,
    pub managed: Option<bool>,
    pub metered: Metered,
    pub active: Option<String>,
    pub ip4: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ip4ConfigProperties {
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WirelessProperties {
    pub access_points: Vec<String>,
    pub active_access_point: Option<String>,
    pub last_scan: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WiredProperties {
    pub carrier: Option<bool>,
    pub speed: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccessPointProperties {
    pub ssid: Option<String>,
    pub raw_ssid: Vec<u8>,
    pub bssid: Option<String>,
    pub strength: u8,
    pub frequency: u32,
    pub flags: u32,
    pub wpa_flags: u32,
    pub rsn_flags: u32,
}

impl AccessPointProperties {
    pub fn security(&self) -> Security {
        Security::read(self.flags, self.wpa_flags, self.rsn_flags)
    }

    pub fn band(&self) -> Band {
        Band::from_frequency(self.frequency)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActiveProperties {
    pub id: Option<String>,
    pub uuid: Option<String>,
    pub kind: Option<String>,
    pub state: ActiveState,
    pub default: Option<bool>,
    pub vpn: Option<bool>,
    pub devices: Vec<String>,
    pub connection: Option<String>,
    pub specific_object: Option<String>,
    pub ip4: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    pub id: Option<String>,
    pub uuid: Option<String>,
    pub kind: Option<String>,
    pub interface: Option<String>,
    pub autoconnect: bool,
    pub ssid: Option<String>,
    pub hidden: bool,
    pub key_mgmt: Option<String>,
    pub seen_bssids: Vec<String>,
    pub timestamp: u64,
}

pub type Settings = HashMap<String, HashMap<String, OwnedValue>>;

fn path(properties: &Properties, key: &str) -> Option<String> {
    match &**properties.get(key)? {
        Value::ObjectPath(value) if value.as_str() != "/" => Some(value.as_str().to_owned()),
        _ => None,
    }
}

fn paths(properties: &Properties, key: &str) -> Vec<String> {
    let Some(value) = properties.get(key) else {
        return Vec::new();
    };
    Vec::<OwnedObjectPath>::try_from(value.clone())
        .map(|found| found.into_iter().map(|one| one.to_string()).collect())
        .unwrap_or_default()
}

fn code(properties: &Properties, key: &str) -> u32 {
    number(properties, key).unwrap_or_default()
}

fn plain(properties: &Properties, key: &str, cap: usize) -> Option<String> {
    optional_clean(<&str>::try_from(properties.get(key)?).ok()?.to_owned(), cap)
}

pub fn decode_ssid(raw: &[u8]) -> Option<String> {
    optional_clean(String::from_utf8_lossy(raw).into_owned(), SSID)
}

fn raw_ssid(properties: &Properties, key: &str) -> Vec<u8> {
    properties
        .get(key)
        .and_then(|value| Vec::<u8>::try_from(value.clone()).ok())
        .map(|mut raw| {
            raw.truncate(SSID_OCTETS);
            raw
        })
        .unwrap_or_default()
}

fn ssid(properties: &Properties, key: &str) -> Option<String> {
    decode_ssid(&raw_ssid(properties, key))
}

pub fn decode_manager(properties: &Properties) -> ManagerProperties {
    ManagerProperties {
        networking_enabled: flag(properties, "NetworkingEnabled"),
        wireless_enabled: flag(properties, "WirelessEnabled"),
        wireless_hardware_enabled: flag(properties, "WirelessHardwareEnabled"),
        connectivity: Connectivity::from_code(code(properties, "Connectivity")),
        metered: Metered::from_code(code(properties, "Metered")),
        primary: path(properties, "PrimaryConnection"),
        devices: paths(properties, "Devices"),
        active: paths(properties, "ActiveConnections"),
    }
}

fn state_reason(properties: &Properties) -> u32 {
    properties
        .get("StateReason")
        .and_then(|value| <(u32, u32)>::try_from(value.clone()).ok())
        .map(|(_, reason)| reason)
        .unwrap_or_default()
}

pub fn decode_device(properties: &Properties) -> DeviceProperties {
    DeviceProperties {
        kind: DeviceKind::from_code(code(properties, "DeviceType")),
        state: DeviceState::from_code(code(properties, "State")),
        reason: state_reason(properties),
        interface: plain(properties, "Interface", INTERFACE),
        managed: flag(properties, "Managed"),
        metered: Metered::from_code(code(properties, "Metered")),
        active: path(properties, "ActiveConnection"),
        ip4: path(properties, "Ip4Config"),
    }
}

/// `AddressData` is `aa{sv}` of `address` and `prefix`. It is the documented shape; `Addresses`
/// beside it is the deprecated packed-integer one and is byte-order dependent.
pub fn decode_ip4_config(properties: &Properties) -> Ip4ConfigProperties {
    let Some(value) = properties.get("AddressData") else {
        return Ip4ConfigProperties::default();
    };
    let Ok(entries) = Vec::<HashMap<String, OwnedValue>>::try_from(value.clone()) else {
        return Ip4ConfigProperties::default();
    };
    Ip4ConfigProperties {
        addresses: entries
            .iter()
            .filter_map(|entry| {
                let address = <&str>::try_from(entry.get("address")?).ok()?;
                let prefix = entry
                    .get("prefix")
                    .and_then(|one| u32::try_from(one).ok())?;
                optional_clean(format!("{address}/{prefix}"), INTERFACE)
            })
            .collect(),
    }
}

pub fn decode_wireless(properties: &Properties) -> WirelessProperties {
    WirelessProperties {
        access_points: paths(properties, "AccessPoints"),
        active_access_point: path(properties, "ActiveAccessPoint"),
        last_scan: properties
            .get("LastScan")
            .and_then(|value| i64::try_from(value).ok()),
    }
}

pub fn decode_wired(properties: &Properties) -> WiredProperties {
    WiredProperties {
        carrier: flag(properties, "Carrier"),
        speed: number(properties, "Speed"),
    }
}

pub fn decode_access_point(properties: &Properties) -> AccessPointProperties {
    AccessPointProperties {
        ssid: ssid(properties, "Ssid"),
        raw_ssid: raw_ssid(properties, "Ssid"),
        bssid: plain(properties, "HwAddress", INTERFACE),
        strength: properties
            .get("Strength")
            .and_then(|value| u8::try_from(value).ok())
            .unwrap_or_default()
            .min(100),
        frequency: code(properties, "Frequency"),
        flags: code(properties, "Flags"),
        wpa_flags: code(properties, "WpaFlags"),
        rsn_flags: code(properties, "RsnFlags"),
    }
}

pub fn decode_active(properties: &Properties) -> ActiveProperties {
    ActiveProperties {
        id: plain(properties, "Id", ID),
        uuid: plain(properties, "Uuid", ID),
        kind: plain(properties, "Type", ID),
        state: ActiveState::from_code(code(properties, "State")),
        default: flag(properties, "Default"),
        vpn: flag(properties, "Vpn"),
        devices: paths(properties, "Devices"),
        connection: path(properties, "Connection"),
        specific_object: path(properties, "SpecificObject"),
        ip4: path(properties, "Ip4Config"),
    }
}

fn setting<'a>(settings: &'a Settings, group: &str, key: &str) -> Option<&'a OwnedValue> {
    settings.get(group)?.get(key)
}

fn setting_text(settings: &Settings, group: &str, key: &str, cap: usize) -> Option<String> {
    optional_clean(
        <&str>::try_from(setting(settings, group, key)?)
            .ok()?
            .to_owned(),
        cap,
    )
}

pub fn decode_profile(settings: &Settings) -> Profile {
    Profile {
        id: setting_text(settings, "connection", "id", ID),
        uuid: setting_text(settings, "connection", "uuid", ID),
        kind: setting_text(settings, "connection", "type", ID),
        interface: setting_text(settings, "connection", "interface-name", INTERFACE),
        autoconnect: setting(settings, "connection", "autoconnect")
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or(true),
        ssid: setting(settings, "802-11-wireless", "ssid")
            .and_then(|value| Vec::<u8>::try_from(value.clone()).ok())
            .and_then(|raw| decode_ssid(&raw)),
        hidden: setting(settings, "802-11-wireless", "hidden")
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or_default(),
        key_mgmt: setting_text(settings, "802-11-wireless-security", "key-mgmt", ID),
        seen_bssids: setting(settings, "802-11-wireless", "seen-bssids")
            .and_then(|value| Vec::<String>::try_from(value.clone()).ok())
            .unwrap_or_default(),
        timestamp: setting(settings, "connection", "timestamp")
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or_default(),
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
pub trait NetworkManager {
    fn get_devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    fn activate_connection(
        &self,
        connection: &ObjectPath<'_>,
        device: &ObjectPath<'_>,
        specific_object: &ObjectPath<'_>,
    ) -> zbus::Result<OwnedObjectPath>;
    fn add_and_activate_connection2(
        &self,
        connection: HashMap<&str, HashMap<&str, Value<'_>>>,
        device: &ObjectPath<'_>,
        specific_object: &ObjectPath<'_>,
        options: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<(
        OwnedObjectPath,
        OwnedObjectPath,
        HashMap<String, OwnedValue>,
    )>;
    fn deactivate_connection(&self, active_connection: &ObjectPath<'_>) -> zbus::Result<()>;
    fn enable(&self, enable: bool) -> zbus::Result<()>;
    fn check_connectivity(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn wireless_enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_wireless_enabled(&self, value: bool) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait Device {
    fn disconnect(&self) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Device.Wireless",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait DeviceWireless {
    fn get_all_access_points(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    fn request_scan(&self, options: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.VPN.Connection",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait VpnConnection {
    #[zbus(property, name = "VpnState")]
    fn vpn_state(&self) -> zbus::Result<u32>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Settings",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager/Settings"
)]
pub trait NetworkSettings {
    fn list_connections(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    fn add_connection(
        &self,
        connection: HashMap<&str, HashMap<&str, Value<'_>>>,
    ) -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Settings.Connection",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait SettingsConnection {
    fn get_settings(&self) -> zbus::Result<Settings>;
    fn get_secrets(&self, setting_name: &str) -> zbus::Result<Settings>;
    fn update(&self, settings: HashMap<&str, HashMap<&str, Value<'_>>>) -> zbus::Result<()>;
    fn update2(
        &self,
        settings: HashMap<&str, HashMap<&str, Value<'_>>>,
        flags: u32,
        args: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn save(&self) -> zbus::Result<()>;
    fn delete(&self) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.AgentManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager/AgentManager"
)]
pub trait AgentManager {
    fn register_with_capabilities(&self, identifier: &str, capabilities: u32) -> zbus::Result<()>;
    fn unregister(&self) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn properties(pairs: Vec<(&str, OwnedValue)>) -> Properties {
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    fn owned<'a, T: Into<Value<'a>>>(value: T) -> OwnedValue {
        OwnedValue::try_from(value.into()).expect("a representable value")
    }

    #[test]
    fn the_object_manager_is_not_where_bluez_puts_it() {
        assert_eq!(
            OBJECTS, "/org/freedesktop",
            "measured: / and /org answer UnknownMethod and /org/freedesktop/NetworkManager \
             answers UnknownInterface"
        );
    }

    #[test]
    fn a_state_reason_takes_the_second_element_because_the_first_repeats_the_state() {
        let decoded = decode_device(&properties(vec![
            ("State", owned(120u32)),
            ("StateReason", owned((120u32, 53u32))),
        ]));

        assert_eq!(decoded.state, DeviceState::Failed);
        assert_eq!(decoded.reason, 53, "element 0 repeats State");
    }

    #[test]
    fn a_missing_state_reason_decodes_rather_than_failing() {
        assert_eq!(decode_device(&properties(vec![])).reason, 0);
    }

    #[test]
    fn every_measured_access_point_decodes_to_the_security_it_advertises() {
        assert_eq!(Security::read(3, 0, 392), Security::Wpa2);
        assert_eq!(Security::read(3, 324, 332), Security::Wpa2);
        assert_eq!(Security::read(3, 324, 0), Security::Wpa);
        assert_eq!(Security::read(1, 0, 0), Security::Wep);
        assert_eq!(Security::read(0, 0, 0), Security::Open);
    }

    #[test]
    fn enterprise_outranks_every_other_key_management_bit() {
        assert_eq!(
            Security::read(3, 0, SEC_KEY_MGMT_PSK | SEC_KEY_MGMT_802_1X),
            Security::Enterprise,
            "an AP offering both must not be offered a psk box"
        );
        assert!(Security::Enterprise.is_enterprise());
    }

    #[test]
    fn sae_is_wpa3_and_owe_needs_no_secret() {
        assert_eq!(Security::read(3, 0, SEC_KEY_MGMT_SAE), Security::Wpa3);
        assert_eq!(Security::read(1, 0, SEC_KEY_MGMT_OWE), Security::Owe);
        assert!(!Security::Owe.needs_a_secret());
        assert!(!Security::Open.needs_a_secret());
        assert!(Security::Wpa2.needs_a_secret());
    }

    #[test]
    fn an_ssid_that_is_not_utf8_still_decodes_and_an_empty_one_is_none() {
        assert_eq!(decode_ssid(b"Skylink").as_deref(), Some("Skylink"));
        assert_eq!(decode_ssid(&[]), None, "one AP here beacons zero bytes");
        assert!(
            decode_ssid(&[0xff, 0xfe, b'h', b'i']).is_some(),
            "invalid UTF-8 is lossy-decoded, never dropped and never a panic"
        );
    }

    #[test]
    fn a_long_ssid_is_capped_by_characters_and_not_by_bytes() {
        let ssid = "Kaffeehaus Freies WLAN Gäste-Zugang Bitte Registrieren".repeat(4);
        let decoded = decode_ssid(ssid.as_bytes()).expect("a name");

        assert!(
            decoded.chars().count() <= SSID + 1,
            "the cap counts characters; the one extra is the ellipsis"
        );
        assert!(
            decoded.ends_with('…'),
            "a truncated name must say so rather than look like a shorter network"
        );
        assert!(decoded.starts_with("Kaffeehaus"));
    }

    #[test]
    fn the_device_filter_keeps_only_what_a_user_can_act_on() {
        for (code, kind, wanted) in [
            (1, DeviceKind::Ethernet, true),
            (2, DeviceKind::Wifi, true),
            (8, DeviceKind::Modem, true),
            (5, DeviceKind::Bluetooth, false),
            (13, DeviceKind::Bridge, false),
            (20, DeviceKind::Veth, false),
            (30, DeviceKind::WifiP2p, false),
            (32, DeviceKind::Loopback, false),
        ] {
            assert_eq!(DeviceKind::from_code(code), kind);
            assert_eq!(kind.user_facing(), wanted, "{kind:?}");
        }
    }

    #[test]
    fn autoconnect_defaults_to_on_because_nm_omits_the_key_when_it_is_true() {
        let bare: Settings = HashMap::from([(
            "connection".to_owned(),
            HashMap::from([("id".to_owned(), owned("Skylink"))]),
        )]);
        assert!(
            decode_profile(&bare).autoconnect,
            "both saved Wi-Fi profiles on the test machine omit the key"
        );

        let off: Settings = HashMap::from([(
            "connection".to_owned(),
            HashMap::from([("autoconnect".to_owned(), owned(false))]),
        )]);
        assert!(!decode_profile(&off).autoconnect);
    }

    #[test]
    fn a_saved_profile_carries_key_mgmt_and_never_a_psk() {
        let settings: Settings = HashMap::from([
            (
                "connection".to_owned(),
                HashMap::from([
                    ("id".to_owned(), owned("Skylink")),
                    ("type".to_owned(), owned("802-11-wireless")),
                ]),
            ),
            (
                "802-11-wireless-security".to_owned(),
                HashMap::from([("key-mgmt".to_owned(), owned("wpa-psk"))]),
            ),
        ]);
        let profile = decode_profile(&settings);

        assert_eq!(profile.key_mgmt.as_deref(), Some("wpa-psk"));
        assert_eq!(profile.id.as_deref(), Some("Skylink"));
        assert!(!profile.hidden);
    }

    #[test]
    fn the_five_strength_bands_split_where_the_icons_do() {
        assert_eq!(Strength::band(94), Strength::Excellent);
        assert_eq!(Strength::band(70), Strength::Good);
        assert_eq!(Strength::band(60), Strength::Good);
        assert_eq!(Strength::band(42), Strength::Ok);
        assert_eq!(Strength::band(20), Strength::Weak);
        assert_eq!(Strength::band(0), Strength::None);
    }

    #[test]
    fn a_band_comes_from_the_frequency_and_an_absent_one_is_unknown() {
        assert_eq!(Band::from_frequency(2412), Band::TwoPointFour);
        assert_eq!(Band::from_frequency(2472), Band::TwoPointFour);
        assert_eq!(Band::from_frequency(5180), Band::Five);
        assert_eq!(Band::from_frequency(5560), Band::Five);
        assert_eq!(Band::from_frequency(6155), Band::Six);
        assert_eq!(Band::from_frequency(0), Band::Unknown);
    }

    #[test]
    fn an_address_comes_from_address_data_and_carries_its_prefix() {
        let mut entry: HashMap<String, OwnedValue> = HashMap::new();
        entry.insert("address".to_owned(), owned("192.168.50.27"));
        entry.insert("prefix".to_owned(), owned(24u32));
        let decoded = decode_ip4_config(&properties(vec![(
            "AddressData",
            owned(vec![entry].as_slice()),
        )]));

        assert_eq!(decoded.addresses, vec!["192.168.50.27/24".to_owned()]);
    }

    #[test]
    fn a_config_with_no_address_data_decodes_to_nothing_rather_than_failing() {
        assert!(decode_ip4_config(&properties(vec![])).addresses.is_empty());
    }

    #[test]
    fn a_name_that_is_not_utf8_keeps_the_bytes_it_beacons() {
        let decoded = decode_access_point(&properties(vec![(
            "Ssid",
            owned(&[0xffu8, 0xfe, 0x78][..]),
        )]));

        assert_eq!(
            decoded.raw_ssid,
            vec![0xffu8, 0xfe, 0x78],
            "activation joins by these bytes; re-encoding the lossy text joins a different network"
        );
        assert_ne!(
            decoded.ssid.unwrap_or_default().into_bytes(),
            decoded.raw_ssid
        );
    }

    #[test]
    fn a_name_longer_than_a_beacon_can_carry_is_cut_to_the_octets_that_fit() {
        let decoded = decode_access_point(&properties(vec![("Ssid", owned(&b"K".repeat(64)[..]))]));

        assert_eq!(decoded.raw_ssid.len(), SSID_OCTETS);
    }

    #[test]
    fn an_access_point_decodes_from_the_values_measured_on_the_live_bus() {
        let decoded = decode_access_point(&properties(vec![
            ("Ssid", owned(&b"Skylink"[..])),
            ("HwAddress", owned("C8:7F:54:98:EA:CC")),
            ("Strength", owned(0x45u8)),
            ("Frequency", owned(5180u32)),
            ("Flags", owned(3u32)),
            ("WpaFlags", owned(0u32)),
            ("RsnFlags", owned(392u32)),
        ]));

        assert_eq!(decoded.ssid.as_deref(), Some("Skylink"));
        assert_eq!(decoded.strength, 69);
        assert_eq!(decoded.security(), Security::Wpa2);
        assert_eq!(decoded.band(), Band::Five);
    }

    #[test]
    fn a_manager_snapshot_drops_the_root_path_that_means_nothing_is_set() {
        let decoded = decode_manager(&properties(vec![
            ("Connectivity", owned(4u32)),
            ("Metered", owned(4u32)),
            (
                "PrimaryConnection",
                owned(ObjectPath::try_from("/").unwrap()),
            ),
        ]));

        assert_eq!(decoded.connectivity, Connectivity::Full);
        assert_eq!(decoded.metered, Metered::GuessNo);
        assert_eq!(decoded.primary, None, "/ is NM's way of saying none");
        assert!(!decoded.metered.marked());
    }

    #[test]
    fn a_guessed_meter_is_still_marked_but_a_guessed_unmetered_one_is_not() {
        assert!(Metered::from_code(1).marked());
        assert!(Metered::from_code(3).marked(), "a guess is still a mark");
        assert!(!Metered::from_code(2).marked());
        assert!(!Metered::from_code(4).marked());
        assert!(!Metered::from_code(0).marked());
    }

    #[test]
    fn an_unknown_connectivity_is_not_a_portal() {
        assert!(
            Connectivity::from_code(0).reaches_the_internet(),
            "the check being disabled must not render as no internet"
        );
        assert!(Connectivity::from_code(4).reaches_the_internet());
        assert!(!Connectivity::from_code(2).reaches_the_internet());
        assert!(!Connectivity::from_code(3).reaches_the_internet());
    }

    #[test]
    fn every_enum_decodes_an_unknown_code_rather_than_failing() {
        assert_eq!(DeviceKind::from_code(9999), DeviceKind::Other);
        assert_eq!(DeviceState::from_code(9999), DeviceState::Unknown);
        assert_eq!(ActiveState::from_code(9999), ActiveState::Unknown);
        assert_eq!(VpnState::from_code(9999), VpnState::Unknown);
        assert_eq!(Connectivity::from_code(9999), Connectivity::Unknown);
        assert_eq!(Metered::from_code(9999), Metered::Unknown);
    }

    #[test]
    fn a_vpn_reaching_activated_is_not_the_same_as_its_active_connection_doing_so() {
        assert_eq!(VpnState::from_code(5), VpnState::Activated);
        assert_eq!(VpnState::from_code(6), VpnState::Failed);
        assert_eq!(VpnState::from_code(2), VpnState::NeedAuth);
        assert_eq!(ActiveState::from_code(2), ActiveState::Activated);
    }

    #[test]
    fn the_secret_flags_match_the_values_libnm_documents() {
        assert_eq!(SECRET_FLAG_ALLOW_INTERACTION, 0x1);
        assert_eq!(SECRET_FLAG_REQUEST_NEW, 0x2);
        assert_eq!(SECRET_FLAG_USER_REQUESTED, 0x4);
        assert_eq!(AGENT_CAPABILITY_VPN_HINTS, 0x1);
    }
}
