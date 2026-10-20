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
fn only_the_question_networkmanager_is_waiting_on_survives_the_popover_closing() {
    assert!(survives_reopen(Some(&Asking::Agent(SecretRequest {
        name: "Skylink".to_owned(),
        path: "/s/1".to_owned(),
        setting: "802-11-wireless-security".to_owned(),
        retry: false,
    }))));
    assert!(!survives_reopen(Some(&Asking::Name)));
    assert!(!survives_reopen(Some(&Asking::Hidden {
        ssid: "Skylink".to_owned()
    })));
    assert!(!survives_reopen(Some(&Asking::Joining {
        id: NetworkId::new("/ap/1".to_owned()),
        name: "Skylink".to_owned(),
    })));
    assert!(!survives_reopen(None));
}

#[test]
fn a_vpn_token_is_not_held_to_a_wifi_passphrase_length() {
    assert_eq!(
        entered_for("802-11-wireless-security"),
        NetworkEntered::Passphrase
    );
    assert_eq!(entered_for("vpn"), NetworkEntered::Secret);
}

#[test]
fn every_security_the_hidden_page_offers_has_a_label_and_the_first_is_the_open_one() {
    assert_eq!(hidden_choices().len(), HIDDEN_SECURITY.len());
    assert_eq!(
        HIDDEN_SECURITY[0],
        nm::Security::Open,
        "open_choice names index 0, which is what suppresses the password box"
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

fn secured(state: &mut NetworkState, saved: bool, security: nm::Security) {
    state.networks[0].security = security;
    state.networks[0].saved = saved.then(|| NetworkId::new("/s/1"));
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
fn a_join_that_would_need_a_password_asks_before_anything_is_torn_down() {
    let mut state = state(70);
    let id = state.networks[0].id.clone();

    secured(&mut state, false, nm::Security::Wpa2);
    assert_eq!(
        asks_first(&state, &id).as_deref(),
        Some("Skylink"),
        "NetworkManager drops the working connection the moment activation is requested, so a \
             password it would then ask for has to be asked for first"
    );

    secured(&mut state, true, nm::Security::Wpa2);
    assert_eq!(
        asks_first(&state, &id),
        None,
        "a saved profile already has its secret; asking again would be a second prompt"
    );

    secured(&mut state, false, nm::Security::Open);
    assert_eq!(
        asks_first(&state, &id),
        None,
        "an open network needs nothing"
    );

    secured(&mut state, false, nm::Security::Enterprise);
    assert_eq!(
        asks_first(&state, &id),
        None,
        "enterprise needs a certificate and an identity, which one password box cannot collect"
    );

    assert_eq!(
        asks_first(&state, &NetworkId::new("/ap/gone")),
        None,
        "a row whose network went out of range between the render and the tap"
    );
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
