use zbus::zvariant::{ObjectPath, OwnedObjectPath};

use super::{Properties, code, flag, text};

pub const SERVICE: &str = "org.freedesktop.UPower";
pub const PATH: &str = "/org/freedesktop/UPower";
pub const DEVICE_INTERFACE: &str = "org.freedesktop.UPower.Device";
pub const DISPLAY_DEVICE: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
pub const DEVICES: &str = "/org/freedesktop/UPower/devices";

const NAME: usize = 24;
const ICON: usize = 64;
const NATIVE: usize = 32;

#[zbus::proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
pub trait UPower {
    fn enumerate_devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    fn get_display_device(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
    #[zbus(signal)]
    fn device_added(&self, device: ObjectPath<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    fn device_removed(&self, device: ObjectPath<'_>) -> zbus::Result<()>;
}

const KBD_BACKLIGHT_PATH: &str = "/org/freedesktop/UPower/KbdBacklight";

#[zbus::proxy(
    interface = "org.freedesktop.UPower.KbdBacklight",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/KbdBacklight"
)]
pub trait UPowerKbdBacklight {
    fn get_brightness(&self) -> zbus::Result<i32>;
    fn get_max_brightness(&self) -> zbus::Result<i32>;
    fn set_brightness(&self, value: i32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn brightness_changed_with_source(&self, value: i32, source: String) -> zbus::Result<()>;
}

#[derive(Debug, Clone)]
pub struct KbdBacklightSource {
    pub path: OwnedObjectPath,
    pub brightness: u32,
    pub max_brightness: u32,
}

pub async fn kbd_backlight_proxy(
    bus: &zbus::Connection,
) -> zbus::Result<(OwnedObjectPath, UPowerKbdBacklightProxy<'static>)> {
    let path = kbd_backlight_path(bus).await;
    let proxy = UPowerKbdBacklightProxy::builder(bus)
        .path(path.clone())?
        .build()
        .await?;
    Ok((path, proxy))
}

pub async fn discover_kbd_backlight(
    bus: &zbus::Connection,
) -> zbus::Result<Option<KbdBacklightSource>> {
    let (path, proxy) = kbd_backlight_proxy(bus).await?;
    let max_brightness = proxy.get_max_brightness().await?;
    if max_brightness <= 0 {
        return Ok(None);
    }
    let brightness = proxy.get_brightness().await?.max(0) as u32;
    Ok(Some(KbdBacklightSource {
        path,
        brightness,
        max_brightness: max_brightness as u32,
    }))
}

async fn kbd_backlight_path(bus: &zbus::Connection) -> OwnedObjectPath {
    let parent = parent_kbd_backlight_path();
    let Some(xml) = introspect_kbd_backlight(bus).await else {
        return parent;
    };
    child_node_names(&xml)
        .into_iter()
        .find_map(|name| OwnedObjectPath::try_from(format!("{KBD_BACKLIGHT_PATH}/{name}")).ok())
        .unwrap_or(parent)
}

async fn introspect_kbd_backlight(bus: &zbus::Connection) -> Option<String> {
    let introspectable = zbus::fdo::IntrospectableProxy::builder(bus)
        .destination(SERVICE)
        .ok()?
        .path(KBD_BACKLIGHT_PATH)
        .ok()?
        .build()
        .await
        .ok()?;
    introspectable.introspect().await.ok()
}

fn parent_kbd_backlight_path() -> OwnedObjectPath {
    OwnedObjectPath::from(ObjectPath::from_static_str_unchecked(KBD_BACKLIGHT_PATH))
}

fn child_node_names(xml: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<node") {
        rest = &rest[start..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        if let Some(name) = node_name(tag) {
            names.push(name);
        }
        rest = &rest[end + 1..];
    }
    names.sort();
    names
}

fn node_name(tag: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("name={quote}");
        let Some(start) = tag.find(&needle).map(|pos| pos + needle.len()) else {
            continue;
        };
        let end = tag[start..].find(quote)?;
        return Some(tag[start..start + end].to_owned());
    }
    None
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower"
)]
pub trait UPowerDevice {
    fn enable_charge_threshold(&self, enable: bool) -> zbus::Result<()>;
    fn get_history(
        &self,
        kind: &str,
        timespan: u32,
        resolution: u32,
    ) -> zbus::Result<Vec<(u32, f64, u32)>>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeviceKind {
    #[default]
    Unknown,
    LinePower,
    Battery,
    Ups,
    Monitor,
    Mouse,
    Keyboard,
    Pda,
    Phone,
    MediaPlayer,
    Tablet,
    Computer,
    GamingInput,
    Pen,
    Touchpad,
    Modem,
    Network,
    Headset,
    Speakers,
    Headphones,
    Video,
    OtherAudio,
    RemoteControl,
    Printer,
    Scanner,
    Camera,
    Wearable,
    Toy,
    BluetoothGeneric,
}

impl DeviceKind {
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::LinePower,
            2 => Self::Battery,
            3 => Self::Ups,
            4 => Self::Monitor,
            5 => Self::Mouse,
            6 => Self::Keyboard,
            7 => Self::Pda,
            8 => Self::Phone,
            9 => Self::MediaPlayer,
            10 => Self::Tablet,
            11 => Self::Computer,
            12 => Self::GamingInput,
            13 => Self::Pen,
            14 => Self::Touchpad,
            15 => Self::Modem,
            16 => Self::Network,
            17 => Self::Headset,
            18 => Self::Speakers,
            19 => Self::Headphones,
            20 => Self::Video,
            21 => Self::OtherAudio,
            22 => Self::RemoteControl,
            23 => Self::Printer,
            24 => Self::Scanner,
            25 => Self::Camera,
            26 => Self::Wearable,
            27 => Self::Toy,
            28 => Self::BluetoothGeneric,
            _ => Self::Unknown,
        }
    }

    pub fn is_line_power(self) -> bool {
        matches!(self, Self::LinePower)
    }

    pub fn is_internal_supply(self) -> bool {
        matches!(self, Self::Battery | Self::Ups)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChargeState {
    #[default]
    Unknown,
    Charging,
    Discharging,
    Empty,
    Full,
    PendingCharge,
    PendingDischarge,
}

impl ChargeState {
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::Full,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WarningLevel {
    #[default]
    Unknown,
    None,
    Discharging,
    Low,
    Critical,
    Action,
}

impl WarningLevel {
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::None,
            2 => Self::Discharging,
            3 => Self::Low,
            4 => Self::Critical,
            5 => Self::Action,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Technology {
    #[default]
    Unknown,
    LithiumIon,
    LithiumPolymer,
    LithiumIronPhosphate,
    LeadAcid,
    NickelCadmium,
    NickelMetalHydride,
}

impl Technology {
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::LithiumIon,
            2 => Self::LithiumPolymer,
            3 => Self::LithiumIronPhosphate,
            4 => Self::LeadAcid,
            5 => Self::NickelCadmium,
            6 => Self::NickelMetalHydride,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChargeThreshold {
    pub enabled: bool,
    pub start: Option<u32>,
    pub end: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceProperties {
    pub native_path: Option<String>,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub kind: DeviceKind,
    pub power_supply: bool,
    pub online: bool,
    pub is_present: bool,
    pub percentage: u8,
    pub state: ChargeState,
    pub icon_name: Option<String>,
    pub time_to_empty: Option<u32>,
    pub time_to_full: Option<u32>,
    pub energy_mwh: Option<u32>,
    pub energy_full_mwh: Option<u32>,
    pub energy_full_design_mwh: Option<u32>,
    pub energy_rate_mw: Option<u32>,
    pub capacity_pct: Option<u8>,
    pub voltage_mv: Option<u32>,
    pub cycles: Option<u32>,
    pub technology: Technology,
    pub warning: WarningLevel,
    pub charge_threshold: Option<ChargeThreshold>,
}

pub fn is_display_device(path: &str) -> bool {
    path == DISPLAY_DEVICE
}

pub fn decode_device(properties: &Properties) -> DeviceProperties {
    DeviceProperties {
        native_path: text(properties, "NativePath", NATIVE),
        vendor: text(properties, "Vendor", NAME),
        model: text(properties, "Model", NAME),
        serial: text(properties, "Serial", NAME),
        kind: DeviceKind::from_code(code(properties, "Type")),
        power_supply: flag(properties, "PowerSupply").unwrap_or_default(),
        online: flag(properties, "Online").unwrap_or_default(),
        is_present: flag(properties, "IsPresent").unwrap_or_default(),
        percentage: percent(properties, "Percentage"),
        state: ChargeState::from_code(code(properties, "State")),
        icon_name: text(properties, "IconName", ICON),
        time_to_empty: seconds(properties, "TimeToEmpty"),
        time_to_full: seconds(properties, "TimeToFull"),
        energy_mwh: milli(properties, "Energy"),
        energy_full_mwh: milli(properties, "EnergyFull"),
        energy_full_design_mwh: milli(properties, "EnergyFullDesign"),
        energy_rate_mw: milli(properties, "EnergyRate"),
        capacity_pct: percent_opt(properties, "Capacity"),
        voltage_mv: milli(properties, "Voltage"),
        cycles: cycles(properties),
        technology: Technology::from_code(code(properties, "Technology")),
        warning: WarningLevel::from_code(code(properties, "WarningLevel")),
        charge_threshold: threshold(properties),
    }
}

fn float(properties: &Properties, key: &str) -> Option<f64> {
    properties
        .get(key)
        .and_then(|value| f64::try_from(value).ok())
        .filter(|value| value.is_finite())
}

fn percent(properties: &Properties, key: &str) -> u8 {
    float(properties, key)
        .map(|value| value.round().clamp(0.0, 100.0) as u8)
        .unwrap_or_default()
}

fn percent_opt(properties: &Properties, key: &str) -> Option<u8> {
    let value = float(properties, key)?;
    if value <= 0.0 {
        return None;
    }
    Some(value.round().clamp(0.0, 100.0) as u8)
}

fn milli(properties: &Properties, key: &str) -> Option<u32> {
    let value = float(properties, key)?;
    if value <= 0.0 {
        return None;
    }
    Some((value * 1000.0).round() as u32)
}

fn seconds(properties: &Properties, key: &str) -> Option<u32> {
    let value = properties
        .get(key)
        .and_then(|value| i64::try_from(value).ok())?;
    u32::try_from(value).ok().filter(|seconds| *seconds > 0)
}

fn cycles(properties: &Properties) -> Option<u32> {
    let value = properties
        .get("ChargeCycles")
        .and_then(|value| i32::try_from(value).ok())?;
    u32::try_from(value).ok().filter(|cycles| *cycles > 0)
}

fn threshold(properties: &Properties) -> Option<ChargeThreshold> {
    if !flag(properties, "ChargeThresholdSupported").unwrap_or_default() {
        return None;
    }
    Some(ChargeThreshold {
        enabled: flag(properties, "ChargeThresholdEnabled").unwrap_or_default(),
        start: percent_limit(properties, "ChargeStartThreshold"),
        end: percent_limit(properties, "ChargeEndThreshold"),
    })
}

fn percent_limit(properties: &Properties, key: &str) -> Option<u32> {
    let value = code(properties, key);
    (1..=100).contains(&value).then_some(value)
}

#[cfg(test)]
mod tests;
