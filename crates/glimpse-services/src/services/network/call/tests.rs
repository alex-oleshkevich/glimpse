use super::*;

#[test]
fn a_secured_network_carries_a_key_management_setting_and_a_secret_only_when_one_was_typed() {
    let settings = wifi_settings(b"Skylink", nm::Security::Wpa2, false, None);
    let guard = settings
        .get("802-11-wireless-security")
        .expect("a security setting");

    assert_eq!(guard.get("key-mgmt"), Some(&Value::from("wpa-psk")));
    assert!(
        !guard.contains_key("psk"),
        "with nothing typed the agent supplies it, and NetworkManager stores what it returns"
    );

    let asked = wifi_settings(
        b"Skylink",
        nm::Security::Wpa2,
        false,
        Some("hunter2hunter2"),
    );
    assert_eq!(
        asked["802-11-wireless-security"].get("psk"),
        Some(&Value::from("hunter2hunter2")),
        "a password typed before the join travels with the profile, so NetworkManager never \
             tears down the working connection for one it then has to ask about"
    );

    let wep = wifi_settings(b"Ancient", nm::Security::Wep, false, Some("s3cr3"));
    assert_eq!(
        wep["802-11-wireless-security"].get("wep-key0"),
        Some(&Value::from("s3cr3")),
        "WEP keeps its key under its own name"
    );
    assert!(!wep["802-11-wireless-security"].contains_key("psk"));
}

#[test]
fn an_open_network_carries_no_security_setting_at_all() {
    let settings = wifi_settings(b"Kaffeehaus", nm::Security::Open, false, None);
    assert!(!settings.contains_key("802-11-wireless-security"));
}

fn point(security: nm::Security) -> nm::AccessPointProperties {
    let (flags, wpa, rsn) = match security {
        nm::Security::Open => (0, 0, 0),
        nm::Security::Wep => (1, 0, 0),
        nm::Security::Wpa => (1, 324, 0),
        nm::Security::Wpa2 => (1, 0, 392),
        nm::Security::Wpa3 => (1, 0, 0x400),
        nm::Security::Owe => (1, 0, 0x800),
        nm::Security::Enterprise => (1, 0, 0x200),
    };
    nm::AccessPointProperties {
        flags,
        wpa_flags: wpa,
        rsn_flags: rsn,
        ..Default::default()
    }
}

fn saved(key_mgmt: Option<&str>) -> nm::Profile {
    nm::Profile {
        key_mgmt: key_mgmt.map(str::to_owned),
        ..Default::default()
    }
}

#[test]
fn a_profile_only_answers_for_an_access_point_its_key_management_can_join() {
    assert!(fits(&saved(Some("wpa-psk")), &point(nm::Security::Wpa2)));
    assert!(
        !fits(&saved(None), &point(nm::Security::Wpa2)),
        "an open profile sharing the name of a secured network would join without a password \
             and fail"
    );
    assert!(!fits(&saved(Some("wpa-psk")), &point(nm::Security::Open)));
    assert!(fits(&saved(None), &point(nm::Security::Open)));
    assert!(fits(&saved(Some("owe")), &point(nm::Security::Owe)));
    assert!(fits(&saved(Some("none")), &point(nm::Security::Wep)));
    assert!(fits(
        &saved(Some("wpa-eap")),
        &point(nm::Security::Enterprise)
    ));
}

#[test]
fn a_wpa3_access_point_answers_a_wpa2_profile_because_transition_mode_advertises_both() {
    assert!(fits(&saved(Some("sae")), &point(nm::Security::Wpa3)));
    assert!(fits(&saved(Some("wpa-psk")), &point(nm::Security::Wpa3)));
    assert!(!fits(&saved(Some("owe")), &point(nm::Security::Wpa3)));
}

#[test]
fn every_key_management_a_profile_can_carry_is_the_one_the_network_advertises() {
    assert_eq!(key_mgmt(nm::Security::Wpa2), Some("wpa-psk"));
    assert_eq!(key_mgmt(nm::Security::Wpa), Some("wpa-psk"));
    assert_eq!(key_mgmt(nm::Security::Wpa3), Some("sae"));
    assert_eq!(key_mgmt(nm::Security::Wep), Some("none"));
    assert_eq!(
        key_mgmt(nm::Security::Owe),
        Some("owe"),
        "an encrypted open network needs the setting even though it needs no password"
    );
    assert_eq!(key_mgmt(nm::Security::Open), None);
    assert_eq!(
        key_mgmt(nm::Security::Enterprise),
        None,
        "802.1X written as a PSK profile is refused by NetworkManager"
    );
    assert!(!joinable(nm::Security::Enterprise));
    assert!(joinable(nm::Security::Owe));
}

#[test]
fn an_encrypted_open_network_carries_its_key_management_and_no_secret() {
    let settings = wifi_settings(b"Cafe", nm::Security::Owe, false, None);
    let guard = settings
        .get("802-11-wireless-security")
        .expect("owe needs the setting");
    assert_eq!(guard.get("key-mgmt"), Some(&Value::from("owe")));
    assert!(!guard.contains_key("psk"));
}

#[test]
fn wpa3_asks_for_sae_rather_than_psk() {
    let settings = wifi_settings(b"Modern", nm::Security::Wpa3, false, None);
    assert_eq!(
        settings["802-11-wireless-security"].get("key-mgmt"),
        Some(&Value::from("sae"))
    );
}

#[test]
fn a_hidden_network_says_so_in_its_profile() {
    let settings = wifi_settings(b"Skylink Guest", nm::Security::Wpa2, true, None);
    assert_eq!(
        settings["802-11-wireless"].get("hidden"),
        Some(&Value::from(true))
    );

    let visible = wifi_settings(b"Skylink", nm::Security::Wpa2, false, None);
    assert!(
        !visible["802-11-wireless"].contains_key("hidden"),
        "a visible network must not be marked hidden"
    );
}

#[test]
fn an_ssid_goes_on_the_wire_as_bytes_and_not_as_a_string() {
    let raw: &[u8] = &[0x53, 0x6b, 0x79, 0x6c, 0x69, 0x6e, 0x6b];
    let settings = wifi_settings(raw, nm::Security::Wpa2, false, None);
    assert_eq!(
        settings["802-11-wireless"].get("ssid"),
        Some(&Value::from(raw))
    );
}
