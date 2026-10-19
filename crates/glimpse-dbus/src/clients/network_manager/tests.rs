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
