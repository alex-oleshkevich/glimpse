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
fn an_lc3_transport_is_named_without_a_vendor_header() {
    let configuration = [0x02, 0x01, 0x08, 0x02, 0x02, 0x01, 0x03, 0x04, 0x78, 0x00];

    assert_eq!(codec(0x06, Some(&configuration)), Codec::Lc3);
    assert_eq!(codec(0x06, None), Codec::Lc3);
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
