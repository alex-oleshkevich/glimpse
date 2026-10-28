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
