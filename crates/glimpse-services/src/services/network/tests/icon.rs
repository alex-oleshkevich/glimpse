use super::*;

#[test]
fn no_managed_device_renders_no_chip_at_all() {
    let bare = NetworkState {
        networking: true,
        ..NetworkState::default()
    };
    assert_eq!(
        bare.icon_name(),
        None,
        "the group hides itself rather than showing a placeholder"
    );
}

#[test]
fn each_of_the_five_bands_has_its_own_icon() {
    let icons: Vec<&str> = [0u8, 20, 42, 70, 94]
        .into_iter()
        .map(|strength| wifi_connected(strength).icon_name().expect("a chip"))
        .collect();
    let unique: std::collections::BTreeSet<&&str> = icons.iter().collect();

    assert_eq!(unique.len(), 5, "got {icons:?}");
    assert_eq!(icons[4], "network-wireless-signal-excellent-symbolic");
    assert_eq!(icons[0], "network-wireless-signal-none-symbolic");
}

#[test]
fn hard_blocked_and_soft_off_are_different_icons() {
    let soft = NetworkState {
        wifi: Some(Radio {
            enabled: false,
            hardware_enabled: true,
        }),
        ..wifi_connected(70)
    };
    let hard = NetworkState {
        wifi: Some(Radio {
            enabled: false,
            hardware_enabled: false,
        }),
        ..wifi_connected(70)
    };

    assert_eq!(soft.icon_name(), Some(DISABLED));
    assert_eq!(hard.icon_name(), Some(BLOCKED));
    assert_ne!(soft.icon_name(), hard.icon_name());
}

#[test]
fn no_internet_replaces_the_strength_icon_rather_than_tinting_it() {
    let portal = NetworkState {
        connectivity: nm::Connectivity::Portal,
        ..wifi_connected(94)
    };
    assert_eq!(portal.icon_name(), Some(NO_ROUTE));

    let unchecked = NetworkState {
        connectivity: nm::Connectivity::Unknown,
        ..wifi_connected(94)
    };
    assert_eq!(
        unchecked.icon_name(),
        Some("network-wireless-signal-excellent-symbolic"),
        "a disabled connectivity check is not a portal"
    );
}

#[test]
fn vpn_is_a_separate_chip_and_only_when_it_is_up() {
    let mut state = wifi_connected(70);
    assert_eq!(state.vpn_icon_name(true), None);

    state.vpn = vec![Vpn {
        id: NetworkId::new("/s/1"),
        name: "Mullvad".to_owned(),
        kind: "wireguard".to_owned(),
        state: nm::VpnState::Activated,
        address: None,
        active: true,
        failure: None,
        busy: None,
    }];
    assert_eq!(state.vpn_icon_name(true), Some(VPN));
    assert_eq!(
        state.vpn_icon_name(false),
        None,
        "the config can turn the second chip off"
    );
    assert_eq!(
        state.icon_name(),
        Some("network-wireless-signal-good-symbolic"),
        "the Wi-Fi chip is unchanged; the VPN does not overlay it"
    );
}

#[test]
fn networking_off_outranks_everything_else() {
    let state = NetworkState {
        networking: false,
        ..wifi_connected(94)
    };
    assert_eq!(state.icon_name(), Some(OFFLINE));
}
