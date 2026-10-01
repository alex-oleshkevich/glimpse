use super::*;
use zbus::zvariant::{OwnedValue, Value};

fn owned<'a, T: Into<Value<'a>>>(value: T) -> OwnedValue {
    OwnedValue::try_from(value.into()).expect("a representable value")
}

fn properties(pairs: Vec<(&str, OwnedValue)>) -> Properties {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

#[test]
fn a_measured_device_decodes() {
    let decoded = decode_device(&properties(vec![
        ("name", owned("Pixel 10 Pro")),
        ("type", owned("phone")),
        ("isReachable", owned(true)),
        ("isPaired", owned(true)),
        ("pairState", owned(3i32)),
    ]));

    assert_eq!(
        decoded,
        DeviceProperties {
            name: Some("Pixel 10 Pro".to_owned()),
            kind: DeviceType::Phone,
            reachable: true,
            pair: PairState::Paired,
        }
    );
}

#[test]
fn every_pair_state_the_daemon_sends_decodes() {
    for (raw, expected) in [
        (0, PairState::NotPaired),
        (1, PairState::Requested),
        (2, PairState::RequestedByPeer),
        (3, PairState::Paired),
        (9, PairState::NotPaired),
    ] {
        let decoded = decode_device(&properties(vec![("pairState", owned(raw))]));
        assert_eq!(decoded.pair, expected, "pairState {raw}");
    }
}

#[test]
fn an_older_daemon_without_pair_state_falls_back_to_is_paired() {
    let decoded = decode_device(&properties(vec![("isPaired", owned(true))]));
    assert_eq!(decoded.pair, PairState::Paired);
}

#[test]
fn an_empty_map_is_an_unreachable_unpaired_unknown() {
    assert_eq!(
        decode_device(&Properties::new()),
        DeviceProperties::default()
    );
}

#[test]
fn a_hostile_name_is_cleaned_and_capped() {
    let decoded = decode_device(&properties(vec![(
        "name",
        owned(format!("<b>{}</b>\n", "x".repeat(4096))),
    )]));
    let name = decoded.name.expect("a name survives cleaning");
    assert_eq!(name.chars().count(), NAME + 1, "the cap plus its ellipsis");
    assert!(!name.contains('\n'));
}

#[test]
fn a_measured_battery_decodes() {
    let decoded = decode_battery(&properties(vec![
        ("charge", owned(76i32)),
        ("isCharging", owned(false)),
        ("hasBattery", owned(true)),
        ("iconName", owned("battery-full-symbolic")),
    ]));

    assert_eq!(
        decoded,
        Some(BatteryProperties {
            charge: Some(76),
            charging: false,
            low: false,
        })
    );
}

#[test]
fn low_comes_from_the_daemons_icon_and_never_while_charging() {
    let low = |icon: &str, charging: bool| {
        decode_battery(&properties(vec![
            ("charge", owned(9i32)),
            ("isCharging", owned(charging)),
            ("iconName", owned(icon)),
        ]))
        .expect("a battery")
        .low
    };
    assert!(low("battery-caution-symbolic", false));
    assert!(low("battery-empty-symbolic", false));
    assert!(!low("battery-low-symbolic", false));
    assert!(!low("battery-caution-charging-symbolic", true));
}

#[test]
fn no_battery_and_an_out_of_range_charge_are_both_absent() {
    assert_eq!(
        decode_battery(&properties(vec![("hasBattery", owned(false))])),
        None
    );
    let decoded = decode_battery(&properties(vec![("charge", owned(-1i32))]))
        .expect("hasBattery absent is not a refusal");
    assert_eq!(decoded.charge, None);
    let decoded = decode_battery(&properties(vec![("charge", owned(140i32))])).expect("a battery");
    assert_eq!(decoded.charge, None);
}

#[test]
fn a_device_id_is_read_off_its_own_path_and_its_plugins_paths() {
    assert_eq!(device_of("/modules/kdeconnect/devices/b98d"), Some("b98d"));
    assert_eq!(
        device_of("/modules/kdeconnect/devices/b98d/battery"),
        Some("b98d")
    );
    assert_eq!(device_of("/modules/kdeconnect"), None);
    assert_eq!(device_of("/modules/kdeconnect/devices"), None);
    assert_eq!(device_of("/modules/kdeconnect/devicesx/b98d"), None);
}
