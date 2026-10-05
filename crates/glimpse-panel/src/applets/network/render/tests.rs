use super::*;
use glimpse_services::{NetworkId, Radio};

fn access(ssid: &str, strength: u8, active: bool) -> Access {
    Access {
        id: NetworkId::new("/ap/1"),
        ssid: (!ssid.is_empty()).then(|| ssid.to_owned()),
        bssid: None,
        strength,
        band: nm::Band::Five,
        security: nm::Security::Wpa2,
        active,
        saved: None,
        address: None,
        busy: None,
        failure: None,
    }
}

fn connected(strength: u8) -> NetworkState {
    NetworkState {
        networking: true,
        wifi: Some(Radio {
            enabled: true,
            hardware_enabled: true,
        }),
        connectivity: nm::Connectivity::Full,
        networks: vec![access("Skylink", strength, true)],
        ..NetworkState::default()
    }
}

#[test]
fn a_busy_network_shows_the_acquiring_icon_and_never_a_word() {
    let mut busy = connected(70);
    busy.networks[0].busy = Some(glimpse_services::NetworkBusy::Connecting);

    assert_eq!(
        busy.icon_name(),
        Some("network-wireless-acquiring-symbolic")
    );
    assert!(
        !status(&busy).contains("onnecting"),
        "the spinner says it is working; the text must not"
    );
}

#[test]
fn metered_is_named_in_the_tooltip_and_never_on_the_chip() {
    let state = NetworkState {
        metered: nm::Metered::GuessYes,
        ..connected(70)
    };

    let tip = tooltip(&state, None, true).expect("a tooltip");
    assert!(tip.contains(&gettext("Metered connection")));
    assert_eq!(
        state.icon_name(),
        Some("network-wireless-signal-good-symbolic"),
        "metered must not change the icon"
    );

    let off = tooltip(&state, None, false).expect("a tooltip");
    assert!(!off.contains(&gettext("Metered connection")));
}

#[test]
fn the_connected_network_shows_its_address_and_a_beacon_in_range_has_none() {
    let mut state = connected(70);
    state.networks[0].address = Some("192.168.50.27/24".to_owned());
    let mut other = access("Neighbour", 40, false);
    other.id = NetworkId::new("/ap/2");
    state.networks.push(other);
    let id = state.networks[0].id.as_str().to_owned();

    let card = details(&state, &id).expect("a detail card");
    let address = card
        .facts
        .iter()
        .find(|fact| fact.label == "IP address")
        .expect("an address fact");
    assert_eq!(address.value, "192.168.50.27/24");
    let red: Vec<&str> = card
        .lines
        .iter()
        .filter(|line| line.destructive)
        .map(|line| line.action.as_str())
        .collect();
    assert_eq!(
        red,
        [match state.networks[0].saved.is_some() {
            true => "forget",
            false => "disconnect",
        }],
        "a card has one red action, and forgetting outranks disconnecting"
    );

    let other = state.networks[1].id.as_str().to_owned();
    let stranger =
        details(&state, &other).expect("a stranger's card says what the one-line row does not");
    let actions: Vec<&str> = stranger
        .lines
        .iter()
        .map(|line| line.action.as_str())
        .collect();
    assert!(
        actions == ["connect"] && !stranger.facts.is_empty(),
        "a stranger's card describes it and offers only Connect"
    );
}

#[test]
fn a_vpn_names_its_own_address_and_an_inactive_one_has_none() {
    let mut state = connected(70);
    state.vpn = vec![glimpse_services::Vpn {
        id: NetworkId::new("/s/vpn"),
        name: "Glimpse Test VPN".to_owned(),
        kind: "wireguard".to_owned(),
        state: nm::VpnState::Activated,
        address: Some("10.64.0.2/32".to_owned()),
        active: true,
        failure: None,
        busy: None,
    }];

    let card = details(&state, "/s/vpn").expect("a detail card");
    let labels: Vec<&str> = card.facts.iter().map(|fact| fact.label.as_str()).collect();
    assert_eq!(labels, ["Type", "IP address"], "facts come first");
    let actions: Vec<&str> = card.lines.iter().map(|line| line.action.as_str()).collect();
    assert_eq!(actions, ["disconnect-vpn"]);
    assert!(
        card.lines[0].destructive,
        "a lone Disconnect is the card's red action"
    );

    state.vpn[0].active = false;
    state.vpn[0].address = None;
    let idle = details(&state, "/s/vpn").expect("the card names the kind");
    let actions: Vec<&str> = idle.lines.iter().map(|line| line.action.as_str()).collect();
    assert!(
        actions == ["connect"] && idle.facts.len() == 1,
        "a tunnel not in use names what it is and offers Connect"
    );
}

#[test]
fn a_wired_connection_in_use_has_a_card_that_disconnects_and_names_its_address() {
    let mut state = connected(70);
    state.wired = vec![glimpse_services::Wired {
        id: NetworkId::new("/org/freedesktop/NetworkManager/Devices/459"),
        name: "enp104s0f4u1i1".to_owned(),
        carrier: true,
        speed: Some(425),
        active: true,
        address: Some("10.0.0.5/24".to_owned()),
        busy: None,
    }];

    let card = details(&state, "/org/freedesktop/NetworkManager/Devices/459")
        .expect("a wired row must unfold a card like every other row");
    let labels: Vec<&str> = card.facts.iter().map(|fact| fact.label.as_str()).collect();
    assert_eq!(labels, ["Speed", "IP address"]);
    let actions: Vec<&str> = card.lines.iter().map(|line| line.action.as_str()).collect();
    assert_eq!(actions, ["disconnect"]);

    state.wired[0].active = false;
    state.wired[0].address = None;
    let actions = |state: &NetworkState| -> Vec<String> {
        details(state, "/org/freedesktop/NetworkManager/Devices/459")
            .map(|card| card.lines.into_iter().map(|line| line.action).collect())
            .unwrap_or_default()
    };
    assert_eq!(
        actions(&state),
        ["connect"],
        "a cable not in use offers Connect"
    );
    state.wired[0].carrier = false;
    assert!(
        actions(&state).is_empty(),
        "an unplugged cable offers nothing that can only fail"
    );
}

#[test]
fn the_popover_marks_a_metered_connection_whatever_the_tooltip_is_told_to_do() {
    let state = NetworkState {
        metered: nm::Metered::GuessYes,
        ..connected(70)
    };

    let metered = |state: &NetworkState| {
        let (rows, _) = entries(state, 0, false, false);
        let row = rows
            .iter()
            .find(|row| row.selected)
            .expect("the connected row");
        details(state, &row.id)
            .is_some_and(|card| card.facts.iter().any(|fact| fact.value == "Metered"))
    };
    assert!(
        metered(&state),
        "the tooltip setting is about the bar; the card inside the popover says so regardless"
    );

    let unmetered = NetworkState {
        metered: nm::Metered::GuessNo,
        ..connected(70)
    };
    assert!(!metered(&unmetered));
}

#[test]
fn an_unmetered_guess_is_not_marked() {
    let state = NetworkState {
        metered: nm::Metered::GuessNo,
        ..connected(70)
    };
    let tip = tooltip(&state, None, true).expect("a tooltip");
    assert!(!tip.contains(&gettext("Metered connection")));
}

#[test]
fn a_hostile_ssid_is_capped_before_it_reaches_the_bar() {
    let long = "Kaffeehaus Freies WLAN Gäste-Zugang Bitte Registrieren".repeat(3);
    let mut state = connected(70);
    state.networks[0].ssid = Some(long);

    let tip = tooltip(&state, None, false).expect("a tooltip");
    assert!(
        tip.chars().count() < 200,
        "got {} chars",
        tip.chars().count()
    );
}

fn listed(names: &[&str]) -> NetworkState {
    let mut state = connected(70);
    state.networks = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let mut one = access(name, 90 - index as u8, false);
            one.id = NetworkId::new(format!("/ap/{index}"));
            one
        })
        .collect();
    state
}

#[test]
fn a_cap_of_zero_lists_every_network_and_offers_nothing_to_expand() {
    let (rows, others) = entries(&listed(&["a", "b", "c", "d", "e"]), 0, false, false);
    assert_eq!(rows.len(), 5);
    assert_eq!(
        others.rest, None,
        "nothing is behind a row that would show nothing new"
    );
}

#[test]
fn a_capped_list_ends_in_a_row_that_shows_the_rest() {
    let state = listed(&["a", "b", "c", "d", "e"]);

    let (rows, others) = entries(&state, 3, false, false);
    assert_eq!(rows.len(), 3, "the cap is what an open list starts at");
    assert!(
        others.open,
        "with nothing known in range the strangers are the list"
    );
    assert_eq!(
        others.rest,
        Some(ngettext("{count} more network", "{count} more networks", 2).replace("{count}", "2"))
    );

    let (rows, others) = entries(&state, 3, false, true);
    assert_eq!(rows.len(), 5, "showing all lifts the cap");
    assert_eq!(
        others.rest, None,
        "and the header, not a second row, is what closes the list again"
    );

    let (rows, others) = entries(&listed(&["a", "b"]), 3, false, false);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        others.rest, None,
        "nothing is hidden, so there is nothing to offer"
    );
}

#[test]
fn a_saved_network_carries_the_settings_that_only_a_profile_has() {
    let mut state = listed(&["Skylink"]);
    state.networks[0].saved = Some(NetworkId::new("/s/1"));
    state.known = vec![glimpse_services::Saved {
        id: NetworkId::new("/s/1"),
        name: Some("Skylink".to_owned()),
        kind: "802-11-wireless".to_owned(),
        uuid: None,
        autoconnect: true,
        in_range: true,
        active: false,
        busy: None,
    }];

    let saved = details(&state, "/ap/0").expect("the network");
    let actions: Vec<&str> = saved
        .lines
        .iter()
        .map(|line| line.action.as_str())
        .collect();
    assert_eq!(
        actions,
        ["autoconnect", "connect", "forget"],
        "the card describes the network, joins it, then offers what a profile has"
    );
    assert_eq!(
        saved.lines[0].toggle,
        Some(true),
        "the toggle reads the profile, not the access point"
    );

    state.networks[0].saved = None;
    assert!(
        details(&state, "/ap/0")
            .is_some_and(|card| card.lines.iter().all(|line| line.action != "forget")),
        "a network nothing is saved for has nothing to forget"
    );
}

#[test]
fn a_known_network_in_range_collapses_the_strangers() {
    let mut state = listed(&["Skylink", "Skylink 2G", "a", "b", "c"]);
    state.networks[1].active = true;
    state.networks[0].saved = Some(NetworkId::new("/s/0"));
    state.known = vec![glimpse_services::Saved {
        id: NetworkId::new("/s/far"),
        name: Some("Far away".to_owned()),
        kind: "802-11-wireless".to_owned(),
        uuid: None,
        autoconnect: true,
        in_range: false,
        active: false,
        busy: None,
    }];

    let (rows, others) = entries(&state, 8, false, false);
    let wifi: Vec<&str> = rows
        .iter()
        .filter(|row| row.place == Place::Wifi)
        .map(|row| row.title.as_str())
        .collect();
    assert_eq!(
        wifi,
        ["Skylink 2G", "Skylink"],
        "the network in use leads, then every saved one in range"
    );
    assert_eq!(
        rows.iter().filter(|row| row.place == Place::Other).count(),
        3,
        "the strangers are built behind the closed header, so opening it can slide them in"
    );
    assert_eq!(
        others,
        Others {
            count: 3,
            open: false,
            rest: None
        }
    );
    assert!(
        !rows.iter().any(|row| row.title == "Far away"),
        "a saved network out of range is not listed at all"
    );

    let (rows, others) = entries(&state, 8, true, false);
    let strangers: Vec<&Entry> = rows
        .iter()
        .filter(|row| row.place == Place::Other)
        .collect();
    assert_eq!(strangers.len(), 3);
    assert!(others.open, "pressing the header opens it");
}
