use zbus::zvariant::{ObjectPath, OwnedObjectPath};

use super::{Properties, flag, text};

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

    #[zbus(property)]
    fn native_path(&self) -> zbus::Result<String>;

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
    #[zbus(property, name = "Type")]
    fn device_type(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn model(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn icon_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn energy_rate(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn capacity(&self) -> zbus::Result<f64>;
    fn enable_charge_threshold(&self, enable: bool) -> zbus::Result<()>;
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
    pub temperature_mc: Option<i32>,
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
        temperature_mc: temperature(properties),
        technology: Technology::from_code(code(properties, "Technology")),
        warning: WarningLevel::from_code(code(properties, "WarningLevel")),
        charge_threshold: threshold(properties),
    }
}

fn code(properties: &Properties, key: &str) -> u32 {
    properties
        .get(key)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_default()
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

fn temperature(properties: &Properties) -> Option<i32> {
    let value = float(properties, "Temperature")?;
    if value == 0.0 {
        return None;
    }
    Some((value * 1000.0).round() as i32)
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
mod tests {
    use super::*;

    const PARENT_ONLY_DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<node>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect">
      <arg name="xml_data" type="s" direction="out"/>
    </method>
  </interface>
  <interface name="org.freedesktop.DBus.Peer">
    <method name="Ping"/>
  </interface>
  <interface name="org.freedesktop.DBus.Properties">
    <method name="GetAll">
      <arg name="interface_name" type="s" direction="in"/>
      <arg name="properties" type="a{sv}" direction="out"/>
    </method>
  </interface>
</node>"#;

    const PARENT_WITH_CHILD_DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<node>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect">
      <arg name="xml_data" type="s" direction="out"/>
    </method>
  </interface>
  <interface name="org.freedesktop.UPower.KbdBacklight">
    <property name="NativePath" type="s" access="read"/>
  </interface>
  <node name="acme_kbd_backlight"/>
</node>"#;

    #[test]
    fn a_document_with_no_child_nodes_yields_no_names() {
        assert_eq!(child_node_names(PARENT_ONLY_DOCUMENT), Vec::<String>::new());
    }

    #[test]
    fn a_document_with_a_child_node_yields_its_name() {
        assert_eq!(
            child_node_names(PARENT_WITH_CHILD_DOCUMENT),
            vec!["acme_kbd_backlight".to_owned()]
        );
    }

    #[test]
    fn a_child_name_resolves_under_the_parent_path() {
        let path = child_node_names(PARENT_WITH_CHILD_DOCUMENT)
            .into_iter()
            .find_map(|name| OwnedObjectPath::try_from(format!("{KBD_BACKLIGHT_PATH}/{name}")).ok())
            .unwrap_or_else(parent_kbd_backlight_path);

        assert_eq!(
            path.as_str(),
            "/org/freedesktop/UPower/KbdBacklight/acme_kbd_backlight"
        );
    }

    #[test]
    fn no_children_falls_back_to_the_parent_path() {
        let path = child_node_names(PARENT_ONLY_DOCUMENT)
            .into_iter()
            .find_map(|name| OwnedObjectPath::try_from(format!("{KBD_BACKLIGHT_PATH}/{name}")).ok())
            .unwrap_or_else(parent_kbd_backlight_path);

        assert_eq!(path.as_str(), KBD_BACKLIGHT_PATH);
    }

    #[test]
    fn a_single_quoted_name_attribute_is_recognised() {
        let document = r#"<node><node name='acme_kbd_backlight'/></node>"#;

        assert_eq!(
            child_node_names(document),
            vec!["acme_kbd_backlight".to_owned()]
        );
    }

    #[test]
    fn an_open_tag_child_node_is_recognised() {
        let document = r#"<node><node name="acme_kbd_backlight"></node></node>"#;

        assert_eq!(
            child_node_names(document),
            vec!["acme_kbd_backlight".to_owned()]
        );
    }

    #[test]
    fn several_children_are_returned_in_a_stable_sorted_order() {
        let document = r#"<node><node name="zzz"/><node name="aaa"/></node>"#;

        assert_eq!(
            child_node_names(document),
            vec!["aaa".to_owned(), "zzz".to_owned()]
        );
    }

    fn properties(pairs: Vec<(&str, zbus::zvariant::Value<'static>)>) -> Properties {
        pairs
            .into_iter()
            .map(|(key, value)| {
                (
                    key.to_owned(),
                    zbus::zvariant::OwnedValue::try_from(value).expect("a plain value"),
                )
            })
            .collect()
    }

    #[test]
    fn a_device_that_implements_nothing_decodes_to_defaults_rather_than_failing() {
        let decoded = decode_device(&properties(Vec::new()));
        assert_eq!(decoded.kind, DeviceKind::Unknown);
        assert_eq!(decoded.percentage, 0);
        assert!(!decoded.is_present);
        assert!(decoded.time_to_empty.is_none());
        assert!(decoded.cycles.is_none());
        assert!(decoded.charge_threshold.is_none());
        assert!(decoded.icon_name.is_none());
    }

    #[test]
    fn the_live_bat1_dump_keeps_the_fields_upower_actually_filled() {
        let decoded = decode_device(&properties(vec![
            ("NativePath", "BAT1".into()),
            ("Vendor", "ASUS".into()),
            ("Model", "A32-K55".into()),
            ("Serial", "".into()),
            ("Type", 2u32.into()),
            ("PowerSupply", true.into()),
            ("Online", false.into()),
            ("Energy", 87.0415f64.into()),
            ("EnergyFull", 87.0415f64.into()),
            ("EnergyFullDesign", 90.0045f64.into()),
            ("EnergyRate", 0f64.into()),
            ("Voltage", 17.243f64.into()),
            ("ChargeCycles", (-1i32).into()),
            ("TimeToEmpty", 0i64.into()),
            ("TimeToFull", 0i64.into()),
            ("Percentage", 100f64.into()),
            ("Temperature", 0f64.into()),
            ("IsPresent", true.into()),
            ("State", 4u32.into()),
            ("Capacity", 96.708f64.into()),
            ("Technology", 1u32.into()),
            ("WarningLevel", 1u32.into()),
            ("IconName", "battery-full-charged-symbolic".into()),
            ("ChargeStartThreshold", 75u32.into()),
            ("ChargeEndThreshold", 80u32.into()),
            ("ChargeThresholdEnabled", false.into()),
            ("ChargeThresholdSupported", true.into()),
        ]));

        assert_eq!(decoded.native_path.as_deref(), Some("BAT1"));
        assert_eq!(decoded.vendor.as_deref(), Some("ASUS"));
        assert_eq!(decoded.model.as_deref(), Some("A32-K55"));
        assert!(decoded.serial.is_none(), "an empty serial is no serial");
        assert_eq!(decoded.kind, DeviceKind::Battery);
        assert!(decoded.power_supply && decoded.is_present);
        assert_eq!(decoded.percentage, 100);
        assert_eq!(decoded.state, ChargeState::Full);
        assert_eq!(
            decoded.icon_name.as_deref(),
            Some("battery-full-charged-symbolic")
        );
        assert!(decoded.time_to_empty.is_none() && decoded.time_to_full.is_none());
        assert_eq!(decoded.energy_mwh, Some(87_042));
        assert_eq!(decoded.energy_full_mwh, Some(87_042));
        assert_eq!(decoded.energy_full_design_mwh, Some(90_005));
        assert!(decoded.energy_rate_mw.is_none(), "0 W is no rate");
        assert_eq!(decoded.capacity_pct, Some(97));
        assert_eq!(decoded.voltage_mv, Some(17_243));
        assert!(
            decoded.cycles.is_none(),
            "UPower sends -1 when it has no count"
        );
        assert!(decoded.temperature_mc.is_none());
        assert_eq!(decoded.technology, Technology::LithiumIon);
        assert_eq!(decoded.warning, WarningLevel::None);
        assert_eq!(
            decoded.charge_threshold,
            Some(ChargeThreshold {
                enabled: false,
                start: Some(75),
                end: Some(80),
            })
        );
    }

    #[test]
    fn the_display_device_does_not_invent_details_the_composite_omits() {
        let decoded = decode_device(&properties(vec![
            ("NativePath", "".into()),
            ("Vendor", "".into()),
            ("Model", "".into()),
            ("Type", 2u32.into()),
            ("Energy", 87.0415f64.into()),
            ("EnergyFull", 87.0415f64.into()),
            ("EnergyFullDesign", 0f64.into()),
            ("Percentage", 100f64.into()),
            ("IsPresent", true.into()),
            ("State", 4u32.into()),
            ("Capacity", 0f64.into()),
            ("ChargeCycles", 0i32.into()),
            ("IconName", "battery-full-charged-symbolic".into()),
            ("ChargeThresholdSupported", false.into()),
        ]));

        assert!(decoded.vendor.is_none() && decoded.model.is_none());
        assert!(decoded.energy_full_design_mwh.is_none());
        assert!(decoded.capacity_pct.is_none());
        assert!(
            decoded.cycles.is_none(),
            "0 cycles is unknown, not a new pack"
        );
        assert!(decoded.charge_threshold.is_none());
        assert_eq!(decoded.percentage, 100);
        assert_eq!(decoded.state, ChargeState::Full);
    }

    #[test]
    fn an_unspecified_threshold_is_not_a_percent() {
        let decoded = decode_device(&properties(vec![
            ("ChargeThresholdSupported", true.into()),
            ("ChargeThresholdEnabled", false.into()),
            ("ChargeStartThreshold", u32::MAX.into()),
            ("ChargeEndThreshold", 0u32.into()),
        ]));
        assert_eq!(
            decoded.charge_threshold,
            Some(ChargeThreshold {
                enabled: false,
                start: None,
                end: None,
            })
        );
    }

    #[test]
    fn line_power_is_a_kind_and_not_a_battery() {
        let decoded = decode_device(&properties(vec![
            ("NativePath", "ACAD".into()),
            ("Type", 1u32.into()),
            ("Online", true.into()),
            ("IsPresent", false.into()),
            ("IconName", "ac-adapter-symbolic".into()),
        ]));

        assert_eq!(decoded.kind, DeviceKind::LinePower);
        assert!(decoded.kind.is_line_power());
        assert!(decoded.online);
        assert!(!decoded.is_present);
    }

    #[test]
    fn a_positive_time_to_empty_is_kept() {
        let decoded = decode_device(&properties(vec![("TimeToEmpty", 12_000i64.into())]));
        assert_eq!(decoded.time_to_empty, Some(12_000));
    }

    #[test]
    fn the_display_device_path_is_the_composite_not_bat1() {
        assert!(is_display_device(DISPLAY_DEVICE));
        assert!(!is_display_device(
            "/org/freedesktop/UPower/devices/battery_BAT1"
        ));
    }
}
