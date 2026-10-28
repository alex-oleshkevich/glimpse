use std::collections::HashMap;

use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use super::{Properties, code, flag, number, optional_clean, path, text};

pub const SERVICE: &str = "org.freedesktop.NetworkManager";
pub const MANAGER: &str = "/org/freedesktop/NetworkManager";

pub const OBJECTS: &str = "/org/freedesktop";

pub const MANAGER1: &str = "org.freedesktop.NetworkManager";
pub const DEVICE1: &str = "org.freedesktop.NetworkManager.Device";
pub const WIRELESS1: &str = "org.freedesktop.NetworkManager.Device.Wireless";
pub const WIRED1: &str = "org.freedesktop.NetworkManager.Device.Wired";
pub const ACCESS_POINT1: &str = "org.freedesktop.NetworkManager.AccessPoint";
pub const ACTIVE1: &str = "org.freedesktop.NetworkManager.Connection.Active";
pub const VPN1: &str = "org.freedesktop.NetworkManager.VPN.Connection";
pub const IP4CONFIG1: &str = "org.freedesktop.NetworkManager.IP4Config";
pub const SETTINGS_CONNECTION1: &str = "org.freedesktop.NetworkManager.Settings.Connection";

const SSID: usize = 64;
const SSID_OCTETS: usize = 32;
const ID: usize = 64;
const INTERFACE: usize = 32;

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

fn paths(properties: &Properties, key: &str) -> Vec<String> {
    let Some(value) = properties.get(key) else {
        return Vec::new();
    };
    Vec::<OwnedObjectPath>::try_from(value.clone())
        .map(|found| found.into_iter().map(|one| one.to_string()).collect())
        .unwrap_or_default()
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
        interface: text(properties, "Interface", INTERFACE),
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
        bssid: text(properties, "HwAddress", INTERFACE),
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
        id: text(properties, "Id", ID),
        uuid: text(properties, "Uuid", ID),
        kind: text(properties, "Type", ID),
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
    fn request_scan(&self, options: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Settings.Connection",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait SettingsConnection {
    fn get_settings(&self) -> zbus::Result<Settings>;
    fn update2(
        &self,
        settings: HashMap<&str, HashMap<&str, Value<'_>>>,
        flags: u32,
        args: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<HashMap<String, OwnedValue>>;
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
mod tests;
