use std::collections::HashMap;

use zbus::zvariant::{OwnedObjectPath, Value};

pub const ADAPTER1: &str = "org.bluez.Adapter1";
pub const DEVICE1: &str = "org.bluez.Device1";
pub const BATTERY1: &str = "org.bluez.Battery1";
pub const MEDIA_TRANSPORT1: &str = "org.bluez.MediaTransport1";

pub const ROOT: &str = "/";
pub const ADAPTERS: &str = "/org/bluez";
pub const SERVICE: &str = "org.bluez";

#[zbus::proxy(interface = "org.bluez.Adapter1", default_service = "org.bluez")]
pub trait Adapter1 {
    fn start_discovery(&self) -> zbus::Result<()>;
    fn stop_discovery(&self) -> zbus::Result<()>;
    fn remove_device(&self, device: zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;
    fn set_discovery_filter(&self, filter: HashMap<&str, Value<'_>>) -> zbus::Result<()>;

    #[zbus(property)]
    fn pairable(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn address_type(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn class(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn discoverable_timeout(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn pairable_timeout(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn modalias(&self) -> zbus::Result<String>;
    #[zbus(property, name = "UUIDs")]
    fn uuids(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn roles(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn powered(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn power_state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn connectable(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_connectable(&self, value: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_powered(&self, value: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn discoverable(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_discoverable(&self, value: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn discovering(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn alias(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn address(&self) -> zbus::Result<String>;
}

#[zbus::proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
pub trait Device1 {
    fn connect(&self) -> zbus::Result<()>;
    fn disconnect(&self) -> zbus::Result<()>;
    fn pair(&self) -> zbus::Result<()>;
    fn cancel_pairing(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn address(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn alias(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn icon(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn paired(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn bonded(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn trusted(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_trusted(&self, value: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn blocked(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_blocked(&self, value: bool) -> zbus::Result<()>;
    #[zbus(property, name = "RSSI")]
    fn rssi(&self) -> zbus::Result<i16>;
    #[zbus(property, name = "TxPower")]
    fn tx_power(&self) -> zbus::Result<i16>;
    #[zbus(property, name = "Class")]
    fn class(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn appearance(&self) -> zbus::Result<u16>;
    #[zbus(property, name = "UUIDs")]
    fn uuids(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn adapter(&self) -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(interface = "org.bluez.Battery1", default_service = "org.bluez")]
pub trait Battery1 {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<u8>;
    #[zbus(property)]
    fn source(&self) -> zbus::Result<String>;
}

#[zbus::proxy(
    interface = "org.bluez.AgentManager1",
    default_service = "org.bluez",
    default_path = "/org/bluez"
)]
pub trait AgentManager1 {
    fn register_agent(
        &self,
        agent: &zbus::zvariant::ObjectPath<'_>,
        capability: &str,
    ) -> zbus::Result<()>;
    fn unregister_agent(&self, agent: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;
}

const NAME: usize = 64;
const ADDRESS: usize = 32;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Power {
    On,
    #[default]
    Off,
    Enabling,
    Disabling,
    Blocked,
}

impl Power {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "on" => Some(Power::On),
            "off" => Some(Power::Off),
            "off-enabling" => Some(Power::Enabling),
            "on-disabling" => Some(Power::Disabling),
            "off-blocked" => Some(Power::Blocked),
            _ => None,
        }
    }

    pub fn from_powered(powered: bool) -> Self {
        match powered {
            true => Power::On,
            false => Power::Off,
        }
    }

    pub fn is_on(self) -> bool {
        matches!(self, Power::On | Power::Disabling)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeviceIcon {
    Headset,
    Headphones,
    Speakers,
    Keyboard,
    Mouse,
    Gamepad,
    Tablet,
    Phone,
    Computer,
    Display,
    Printer,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Profile {
    Audio,
    Calls,
    RemoteControl,
    Input,
    Network,
    FileTransfer,
    PhoneBook,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Codec {
    Sbc,
    Mp3,
    Aac,
    Atrac,
    Ldac,
    AptX,
    AptXHd,
    Lhdc,
    #[default]
    Vendor,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisconnectReason {
    #[default]
    Unknown,
    Timeout,
    Local,
    Remote,
    Authentication,
    Suspend,
}

impl DisconnectReason {
    pub fn parse(value: &str) -> Self {
        match value.rsplit('.').next().unwrap_or_default() {
            "Timeout" => DisconnectReason::Timeout,
            "Local" => DisconnectReason::Local,
            "Remote" => DisconnectReason::Remote,
            "Authentication" => DisconnectReason::Authentication,
            "Suspend" => DisconnectReason::Suspend,
            _ => DisconnectReason::Unknown,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdapterProperties {
    pub address: Option<String>,
    pub alias: Option<String>,
    pub powered: Option<bool>,
    pub power_state: Option<Power>,
    pub discoverable: Option<bool>,
    pub discovering: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceProperties {
    pub adapter: Option<String>,
    pub address: Option<String>,
    pub alias: Option<String>,
    pub icon: Option<String>,
    pub class: Option<u32>,
    pub paired: Option<bool>,
    pub bonded: Option<bool>,
    pub trusted: Option<bool>,
    pub blocked: Option<bool>,
    pub connected: Option<bool>,
    pub rssi: Option<i16>,
    pub uuids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatteryProperties {
    pub percentage: Option<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransportProperties {
    pub device: Option<String>,
    pub codec: Option<u8>,
    pub configuration: Option<Vec<u8>>,
}

use super::{Properties, flag, text};

fn path(properties: &Properties, key: &str) -> Option<String> {
    match &**properties.get(key)? {
        Value::ObjectPath(value) => Some(value.as_str().to_owned()),
        _ => None,
    }
}

fn strings(properties: &Properties, key: &str) -> Option<Vec<String>> {
    Vec::<String>::try_from(properties.get(key)?.clone()).ok()
}

pub fn decode_adapter(properties: &Properties) -> AdapterProperties {
    AdapterProperties {
        address: text(properties, "Address", ADDRESS),
        alias: text(properties, "Alias", NAME),
        powered: flag(properties, "Powered"),
        power_state: properties
            .get("PowerState")
            .and_then(|value| <&str>::try_from(value).ok())
            .and_then(Power::parse),
        discoverable: flag(properties, "Discoverable"),
        discovering: flag(properties, "Discovering"),
    }
}

pub fn decode_device(properties: &Properties) -> DeviceProperties {
    DeviceProperties {
        adapter: path(properties, "Adapter"),
        address: text(properties, "Address", ADDRESS),
        alias: text(properties, "Alias", NAME),
        icon: text(properties, "Icon", NAME),
        class: properties.get("Class").and_then(|v| u32::try_from(v).ok()),
        paired: flag(properties, "Paired"),
        bonded: flag(properties, "Bonded"),
        trusted: flag(properties, "Trusted"),
        blocked: flag(properties, "Blocked"),
        connected: flag(properties, "Connected"),
        rssi: properties.get("RSSI").and_then(|v| i16::try_from(v).ok()),
        uuids: strings(properties, "UUIDs"),
    }
}

pub fn decode_battery(properties: &Properties) -> BatteryProperties {
    BatteryProperties {
        percentage: properties
            .get("Percentage")
            .and_then(|value| u8::try_from(value).ok())
            .map(|percentage| percentage.min(100)),
    }
}

pub fn decode_transport(properties: &Properties) -> TransportProperties {
    TransportProperties {
        device: path(properties, "Device"),
        codec: properties.get("Codec").and_then(|v| u8::try_from(v).ok()),
        configuration: properties
            .get("Configuration")
            .and_then(|value| Vec::<u8>::try_from(value.clone()).ok()),
    }
}

pub fn is_synthesized_name(alias: &str, address: &str) -> bool {
    alias.len() == address.len()
        && alias
            .bytes()
            .zip(address.bytes())
            .all(|(alias, address)| match address {
                b':' => alias == b'-',
                _ => alias.eq_ignore_ascii_case(&address),
            })
}

pub fn device_icon(icon: Option<&str>, class: Option<u32>) -> DeviceIcon {
    match icon {
        Some("audio-headset") => return DeviceIcon::Headset,
        Some("audio-headphones") => return DeviceIcon::Headphones,
        Some("audio-card" | "audio-speakers") => return DeviceIcon::Speakers,
        Some("input-keyboard") => return DeviceIcon::Keyboard,
        Some("input-mouse") => return DeviceIcon::Mouse,
        Some("input-gaming") => return DeviceIcon::Gamepad,
        Some("input-tablet") => return DeviceIcon::Tablet,
        Some("phone") => return DeviceIcon::Phone,
        Some("computer") => return DeviceIcon::Computer,
        Some("video-display") => return DeviceIcon::Display,
        Some("printer") => return DeviceIcon::Printer,
        _ => (),
    }

    match class.map(|class| (class >> 8) & 0x1f) {
        Some(0x01) => DeviceIcon::Computer,
        Some(0x02) => DeviceIcon::Phone,
        Some(0x04) => DeviceIcon::Speakers,
        Some(0x05) => DeviceIcon::Keyboard,
        Some(0x06) => DeviceIcon::Printer,
        _ => DeviceIcon::Unknown,
    }
}

fn short_uuid(uuid: &str) -> Option<u16> {
    let (short, rest) = uuid.split_once('-')?;
    (rest.eq_ignore_ascii_case("0000-1000-8000-00805f9b34fb") && short.len() == 8)
        .then(|| u32::from_str_radix(short, 16).ok())
        .flatten()
        .and_then(|value| u16::try_from(value).ok())
}

pub fn profiles(uuids: &[String]) -> Vec<Profile> {
    let mut found: Vec<Profile> = uuids
        .iter()
        .filter_map(|uuid| short_uuid(uuid))
        .filter_map(|uuid| match uuid {
            0x110a | 0x110b | 0x110d => Some(Profile::Audio),
            0x1108 | 0x1112 | 0x111e | 0x111f => Some(Profile::Calls),
            0x110e | 0x110f => Some(Profile::RemoteControl),
            0x1124 | 0x1812 => Some(Profile::Input),
            0x1115..=0x1117 => Some(Profile::Network),
            0x1105 | 0x1106 => Some(Profile::FileTransfer),
            0x112f | 0x1130 | 0x1132 => Some(Profile::PhoneBook),
            _ => None,
        })
        .collect();
    found.sort_unstable();
    found.dedup();
    found
}

pub fn codec(codec: u8, configuration: Option<&[u8]>) -> Codec {
    match codec {
        0x00 => return Codec::Sbc,
        0x01 => return Codec::Mp3,
        0x02 => return Codec::Aac,
        0x04 => return Codec::Atrac,
        _ => (),
    }

    let tuple = (|| {
        let bytes = configuration?;
        let company = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
        let identifier = u16::from_le_bytes(bytes.get(4..6)?.try_into().ok()?);
        Some((company, identifier))
    })();

    match tuple {
        Some((0x012d, 0x00aa)) => Codec::Ldac,
        Some((0x004f, 0x0001)) => Codec::AptX,
        Some((0x00d7, 0x0024)) => Codec::AptXHd,
        Some((0x053a, 0x484c)) => Codec::Lhdc,
        _ => Codec::Vendor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::OwnedValue;

    fn properties(pairs: Vec<(&str, Value<'static>)>) -> Properties {
        pairs
            .into_iter()
            .map(|(key, value)| {
                (
                    key.to_owned(),
                    OwnedValue::try_from(value).expect("a plain value"),
                )
            })
            .collect()
    }

    fn bonded() -> Properties {
        properties(vec![
            ("Address", "F8:4E:17:BC:EE:D5".into()),
            ("Alias", "WH-1000XM4".into()),
            ("Name", "WH-1000XM4".into()),
            ("Icon", "audio-headset".into()),
            ("Class", 2_360_324u32.into()),
            ("Paired", true.into()),
            ("Bonded", true.into()),
            ("Trusted", true.into()),
            ("Connected", true.into()),
        ])
    }

    fn discovered() -> Properties {
        properties(vec![
            ("Address", "45:16:94:89:4F:38".into()),
            ("Alias", "45-16-94-89-4F-38".into()),
            ("RSSI", (-73i16).into()),
        ])
    }

    #[test]
    fn a_bonded_device_decodes_without_rssi() {
        let decoded = decode_device(&bonded());

        assert_eq!(decoded.rssi, None);
        assert_eq!(decoded.alias.as_deref(), Some("WH-1000XM4"));
        assert_eq!(decoded.bonded, Some(true));
        assert_eq!(decoded.blocked, None);
    }

    #[test]
    fn a_discovered_device_decodes_without_name_class_or_icon() {
        let decoded = decode_device(&discovered());

        assert_eq!(decoded.icon, None);
        assert_eq!(decoded.class, None);
        assert_eq!(decoded.paired, None);
        assert_eq!(decoded.rssi, Some(-73));
        assert!(is_synthesized_name(
            decoded.alias.as_deref().unwrap_or_default(),
            decoded.address.as_deref().unwrap_or_default()
        ));
    }

    #[test]
    fn a_bonded_name_is_not_synthesized() {
        assert!(!is_synthesized_name("WH-1000XM4", "F8:4E:17:BC:EE:D5"));
    }

    #[test]
    fn an_invalidated_property_decodes_as_absent() {
        let changed = properties(vec![("Connected", false.into())]);
        let decoded = decode_device(&changed);

        assert_eq!(decoded.connected, Some(false));
        assert_eq!(decoded.rssi, None);
    }

    #[test]
    fn an_adapter_without_power_state_falls_back_to_powered() {
        let decoded = decode_adapter(&properties(vec![
            ("Alias", "glimpse".into()),
            ("Powered", true.into()),
            ("Discovering", false.into()),
        ]));

        assert_eq!(decoded.power_state, None);
        assert_eq!(
            decoded
                .power_state
                .unwrap_or_else(|| Power::from_powered(decoded.powered.unwrap_or_default())),
            Power::On
        );
    }

    #[test]
    fn a_blocked_adapter_decodes_its_power_state() {
        let decoded = decode_adapter(&properties(vec![
            ("Powered", false.into()),
            ("PowerState", "off-blocked".into()),
        ]));

        assert_eq!(decoded.power_state, Some(Power::Blocked));
    }

    #[test]
    fn a_hostile_device_name_is_capped_and_cleaned() {
        let decoded = decode_device(&properties(vec![(
            "Alias",
            "[TV] Samsung\u{202e}AU7172".into(),
        )]));

        let alias = decoded.alias.expect("an alias");
        assert!(!alias.contains('\u{202e}'));
        assert!(alias.chars().count() <= NAME + 1);
    }

    #[test]
    fn a_device_with_no_icon_falls_back_to_its_class() {
        assert_eq!(device_icon(None, Some(2_360_324)), DeviceIcon::Speakers);
        assert_eq!(device_icon(Some("audio-card"), None), DeviceIcon::Speakers);
        assert_eq!(device_icon(None, None), DeviceIcon::Unknown);
    }

    #[test]
    fn unknown_uuids_are_dropped_and_known_ones_deduplicated() {
        let uuids = vec![
            "0000110a-0000-1000-8000-00805f9b34fb".to_owned(),
            "0000110b-0000-1000-8000-00805f9b34fb".to_owned(),
            "0000110e-0000-1000-8000-00805f9b34fb".to_owned(),
            "931c7e8a-540f-4686-b798-e8df0a2ad9f7".to_owned(),
        ];

        assert_eq!(
            profiles(&uuids),
            vec![Profile::Audio, Profile::RemoteControl]
        );
    }

    #[test]
    fn a_vendor_codec_is_named_from_its_configuration() {
        let configuration = [0x2d, 0x01, 0x00, 0x00, 0xaa, 0x00, 0x04, 0x01];

        assert_eq!(codec(0xff, Some(&configuration)), Codec::Ldac);
        assert_eq!(codec(0x00, None), Codec::Sbc);
        assert_eq!(codec(0xff, None), Codec::Vendor);
        assert_eq!(codec(0xff, Some(&[0x01, 0x02])), Codec::Vendor);
    }

    #[test]
    fn a_transport_decodes_its_parent_device() {
        let decoded = decode_transport(&properties(vec![
            (
                "Device",
                Value::ObjectPath(
                    "/org/bluez/hci0/dev_F8_4E_17_BC_EE_D5"
                        .try_into()
                        .expect("a path"),
                ),
            ),
            ("Codec", 255u8.into()),
        ]));

        assert_eq!(
            decoded.device.as_deref(),
            Some("/org/bluez/hci0/dev_F8_4E_17_BC_EE_D5")
        );
        assert_eq!(decoded.configuration, None);
    }

    #[test]
    fn a_disconnect_reason_parses_its_last_segment() {
        assert_eq!(
            DisconnectReason::parse("org.bluez.Reason.Local"),
            DisconnectReason::Local
        );
        assert_eq!(
            DisconnectReason::parse("nonsense"),
            DisconnectReason::Unknown
        );
    }
}
