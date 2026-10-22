use super::*;
use glimpse_services::Radio;

#[test]
fn a_wired_row_routes_to_its_device_and_never_to_an_access_point() {
    let mut state = NetworkState::default();
    let wired = NetworkId::new("/org/freedesktop/NetworkManager/Devices/459");
    state.wired = vec![glimpse_services::Wired {
        id: wired.clone(),
        name: "enp104s0f4u1i1".to_owned(),
        carrier: true,
        speed: Some(425),
        active: false,
        address: None,
        busy: None,
    }];

    assert_eq!(
        route(&state, &wired),
        Route::Wired,
        "a wired row carries a device path, which connect_access_point cannot find"
    );
    assert_eq!(route(&state, &NetworkId::new("/ap/1")), Route::AccessPoint);
}

#[test]
fn every_security_the_hidden_page_offers_has_a_label_and_the_first_is_the_open_one() {
    assert_eq!(hidden_choices().len(), HIDDEN_SECURITY.len());
    assert_eq!(
        HIDDEN_SECURITY[0],
        nm::Security::Open,
        "the dropdown starts on the open network, which needs no secret"
    );
    assert!(!HIDDEN_SECURITY.contains(&nm::Security::Enterprise));
}

fn state(strength: u8) -> NetworkState {
    with_strength(
        NetworkState {
            networking: true,
            wifi: Some(Radio {
                enabled: true,
                hardware_enabled: true,
            }),
            connectivity: nm::Connectivity::Full,
            ..NetworkState::default()
        },
        strength,
    )
}

fn with_strength(mut state: NetworkState, strength: u8) -> NetworkState {
    state.networks = vec![glimpse_services::Access {
        id: glimpse_services::NetworkId::new("/ap/1"),
        ssid: Some("Skylink".to_owned()),
        bssid: None,
        strength,
        band: nm::Band::Five,
        security: nm::Security::Wpa2,
        active: true,
        saved: None,
        address: None,
        busy: None,
        failure: None,
    }];
    state
}

#[test]
fn a_row_is_activated_by_the_command_its_own_id_belongs_to() {
    let mut state = state(70);
    let ap = state.networks[0].id.clone();
    state.known = vec![glimpse_services::Saved {
        id: NetworkId::new("/s/1"),
        name: Some("Skylink 2G".to_owned()),
        kind: "802-11-wireless".to_owned(),
        uuid: None,
        autoconnect: true,
        in_range: false,
        active: false,
        busy: None,
    }];
    state.vpn = vec![glimpse_services::Vpn {
        id: NetworkId::new("/s/9"),
        name: "Mullvad".to_owned(),
        kind: "wireguard".to_owned(),
        state: nm::VpnState::Disconnected,
        address: None,
        active: false,
        failure: None,
        busy: None,
    }];

    assert_eq!(route(&state, &ap), Route::AccessPoint);
    assert_eq!(
        route(&state, &NetworkId::new("/s/1")),
        Route::Profile,
        "a known-networks row is a saved profile, which connect_access_point cannot find"
    );
    assert_eq!(route(&state, &NetworkId::new("/s/9")), Route::Vpn);
}

#[test]
fn an_applet_with_no_device_contributes_no_chips() {
    let bare = NetworkState {
        networking: true,
        ..NetworkState::default()
    };
    assert!(bare.icon_name().is_none());
}

#[test]
fn a_connected_vpn_adds_a_second_chip_rather_than_changing_the_first() {
    let mut with_vpn = state(70);
    with_vpn.vpn = vec![glimpse_services::Vpn {
        id: glimpse_services::NetworkId::new("/s/1"),
        name: "Mullvad".to_owned(),
        kind: "wireguard".to_owned(),
        state: nm::VpnState::Activated,
        address: None,
        active: true,
        failure: None,
        busy: None,
    }];

    assert_eq!(with_vpn.icon_name(), state(70).icon_name());
    assert!(with_vpn.vpn_icon_name(true).is_some());
}
