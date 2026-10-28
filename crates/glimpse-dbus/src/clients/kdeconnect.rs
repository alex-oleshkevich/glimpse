use super::{Properties, flag, text};

pub const SERVICE: &str = "org.kde.kdeconnect";
pub const ROOT: &str = "/modules/kdeconnect";
pub const DEVICES: &str = "/modules/kdeconnect/devices";

pub const DEVICE: &str = "org.kde.kdeconnect.device";
pub const BATTERY: &str = "org.kde.kdeconnect.device.battery";
pub const CLIPBOARD: &str = "org.kde.kdeconnect.device.clipboard";

pub const PLUGIN_BATTERY: &str = "kdeconnect_battery";
pub const PLUGIN_RING: &str = "kdeconnect_findmyphone";
pub const PLUGIN_PING: &str = "kdeconnect_ping";
pub const PLUGIN_CLIPBOARD: &str = "kdeconnect_clipboard";
pub const PLUGIN_SHARE: &str = "kdeconnect_share";
pub const PLUGIN_SFTP: &str = "kdeconnect_sftp";
pub const PLUGIN_SMS: &str = "kdeconnect_sms";

const NAME: usize = 64;

#[zbus::proxy(
    interface = "org.kde.kdeconnect.daemon",
    default_path = "/modules/kdeconnect"
)]
pub trait Daemon {
    #[zbus(name = "devices")]
    fn devices(&self, only_reachable: bool, only_paired: bool) -> zbus::Result<Vec<String>>;
}

#[zbus::proxy(interface = "org.kde.kdeconnect.device")]
pub trait Device {
    #[zbus(name = "requestPairing")]
    fn request_pairing(&self) -> zbus::Result<()>;

    #[zbus(name = "unpair")]
    fn unpair(&self) -> zbus::Result<()>;

    #[zbus(name = "loadedPlugins")]
    fn loaded_plugins(&self) -> zbus::Result<Vec<String>>;
}

#[zbus::proxy(interface = "org.kde.kdeconnect.device.findmyphone")]
pub trait FindMyPhone {
    #[zbus(name = "ring")]
    fn ring(&self) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.kde.kdeconnect.device.ping")]
pub trait Ping {
    #[zbus(name = "sendPing")]
    fn send_ping(&self) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.kde.kdeconnect.device.clipboard")]
pub trait Clipboard {
    #[zbus(name = "sendClipboard")]
    fn send_clipboard(&self) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.kde.kdeconnect.device.share")]
pub trait Share {
    #[zbus(name = "shareUrls")]
    fn share_urls(&self, urls: &[&str]) -> zbus::Result<()>;

    #[zbus(name = "shareText")]
    fn share_text(&self, text: &str) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.kde.kdeconnect.device.sftp")]
pub trait Sftp {
    #[zbus(name = "startBrowsing")]
    fn start_browsing(&self) -> zbus::Result<bool>;
}

#[zbus::proxy(interface = "org.kde.kdeconnect.device.sms")]
pub trait Sms {
    #[zbus(name = "launchApp")]
    fn launch_app(&self) -> zbus::Result<()>;
}

pub fn device_path(id: &str) -> String {
    format!("{DEVICES}/{id}")
}

pub fn plugin_path(id: &str, plugin: &str) -> String {
    format!("{DEVICES}/{id}/{plugin}")
}

pub fn device_of(path: &str) -> Option<&str> {
    let id = path.strip_prefix(DEVICES)?.strip_prefix('/')?;
    let id = id.split('/').next()?;
    (!id.is_empty()).then_some(id)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeviceType {
    #[default]
    Unknown,
    Phone,
    Tablet,
    Desktop,
    Laptop,
    Tv,
}

impl DeviceType {
    fn parse(value: &str) -> Self {
        match value {
            "phone" | "smartphone" => DeviceType::Phone,
            "tablet" => DeviceType::Tablet,
            "desktop" => DeviceType::Desktop,
            "laptop" => DeviceType::Laptop,
            "tv" => DeviceType::Tv,
            _ => DeviceType::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PairState {
    #[default]
    NotPaired,
    Requested,
    RequestedByPeer,
    Paired,
}

impl PairState {
    fn from_raw(value: i32) -> Self {
        match value {
            1 => PairState::Requested,
            2 => PairState::RequestedByPeer,
            3 => PairState::Paired,
            _ => PairState::NotPaired,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceProperties {
    pub name: Option<String>,
    pub kind: DeviceType,
    pub reachable: bool,
    pub pair: PairState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BatteryProperties {
    pub charge: Option<u8>,
    pub charging: bool,
    pub low: bool,
}

pub fn decode_device(properties: &Properties) -> DeviceProperties {
    let pair = properties
        .get("pairState")
        .and_then(|value| i32::try_from(value).ok())
        .map(PairState::from_raw)
        .unwrap_or_else(|| match flag(properties, "isPaired") {
            Some(true) => PairState::Paired,
            _ => PairState::NotPaired,
        });
    DeviceProperties {
        name: text(properties, "name", NAME),
        kind: properties
            .get("type")
            .and_then(|value| <&str>::try_from(value).ok())
            .map(DeviceType::parse)
            .unwrap_or_default(),
        reachable: flag(properties, "isReachable").unwrap_or(false),
        pair,
    }
}

pub fn decode_battery(properties: &Properties) -> Option<BatteryProperties> {
    if flag(properties, "hasBattery") == Some(false) {
        return None;
    }
    let charge = properties
        .get("charge")
        .and_then(|value| i32::try_from(value).ok())
        .and_then(|charge| u8::try_from(charge).ok())
        .filter(|charge| *charge <= 100);
    let charging = flag(properties, "isCharging").unwrap_or(false);
    let icon = properties
        .get("iconName")
        .and_then(|value| <&str>::try_from(value).ok())
        .unwrap_or_default();
    let low =
        !charging && (icon.starts_with("battery-caution") || icon.starts_with("battery-empty"));
    Some(BatteryProperties {
        charge,
        charging,
        low,
    })
}

pub fn decode_auto_share_disabled(properties: &Properties) -> bool {
    flag(properties, "isAutoShareDisabled").unwrap_or(false)
}

#[cfg(test)]
mod tests;
