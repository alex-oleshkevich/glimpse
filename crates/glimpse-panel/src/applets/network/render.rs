use gettextrs::{gettext, ngettext};

use glimpse_dbus::network_manager as nm;
use glimpse_services::{Access, NetworkBusy, NetworkFailure, NetworkState};
use glimpse_widgets::{
    NetworkDetails as Details, NetworkEntry as Entry, NetworkLine as Line, NetworkPlace as Place,
};

const LABEL_MAX_CHARS: usize = 32;

pub fn cap(name: &str) -> String {
    glimpse_utils::clean(name, LABEL_MAX_CHARS)
}

pub fn status(state: &NetworkState) -> String {
    if !state.networking {
        return gettext("Networking is off");
    }
    if let Some(wired) = state.wired.iter().find(|one| one.active) {
        let _ = wired;
        return match state.reaches_the_internet() {
            true => gettext("Connected by cable"),
            false => gettext("Connected, no internet"),
        };
    }
    let Some(radio) = state.wifi else {
        return gettext("No network adapter");
    };
    if radio.blocked() {
        return gettext("Wi-Fi is blocked by hardware");
    }
    if !radio.enabled {
        return gettext("Wi-Fi is off");
    }
    match state.connected() {
        Some(network) => {
            let name = network
                .ssid
                .as_deref()
                .map(cap)
                .unwrap_or_else(|| gettext("Unnamed network"));
            match state.reaches_the_internet() {
                true => name,
                false => gettext("{network}: connected, no internet").replace("{network}", &name),
            }
        }
        None => gettext("Not connected"),
    }
}

pub fn tooltip(state: &NetworkState, format: Option<&str>, metered: bool) -> Option<String> {
    let mut lines = vec![status(state)];

    if metered && state.metered.marked() {
        lines.push(gettext("Metered connection"));
    }
    if let Some(network) = state.connected() {
        lines
            .push(gettext("Signal {percent}%").replace("{percent}", &network.strength.to_string()));
    }
    if let Some(vpn) = state.vpn.iter().find(|one| one.active) {
        lines.push(gettext("VPN: {name}").replace("{name}", &cap(&vpn.name)));
    }

    let joined = lines.join("\n");
    let Some(format) = format else {
        return Some(joined);
    };

    let plain = status(state);
    let signal = state
        .connected()
        .map(|network| network.strength.to_string())
        .unwrap_or_default();
    Some(crate::applets::tokens::render(
        format,
        |token| match token {
            "status" => Some(plain.as_str()),
            "signal" => Some(signal.as_str()),
            _ => None,
        },
    ))
}

pub fn wording(failure: NetworkFailure) -> Option<String> {
    Some(match failure {
        NetworkFailure::NeedSecrets => gettext("That network needs a password."),
        NetworkFailure::WrongKey => gettext("The password was not accepted."),
        NetworkFailure::NotFound => gettext("That network is no longer in range."),
        NetworkFailure::Timeout => gettext("The connection timed out."),
        NetworkFailure::ConfigFailed => gettext("The connection could not be configured."),
        NetworkFailure::Dropped => gettext("The connection was dropped."),
        NetworkFailure::Removed => gettext("That network has been forgotten."),
        NetworkFailure::NoDevice => gettext("There is no network device to use."),
        NetworkFailure::Refused => gettext("NetworkManager refused that."),
        NetworkFailure::Unknown => gettext("That did not work."),
    })
}

pub fn security(security: nm::Security) -> String {
    match security {
        nm::Security::Open => gettext("Open"),
        nm::Security::Wep => "WEP".to_owned(),
        nm::Security::Wpa => "WPA".to_owned(),
        nm::Security::Wpa2 => "WPA2".to_owned(),
        nm::Security::Wpa3 => "WPA3".to_owned(),
        nm::Security::Enterprise => gettext("Enterprise"),
        nm::Security::Owe => gettext("Open, encrypted"),
    }
}

fn band(band: nm::Band) -> Option<String> {
    match band {
        nm::Band::Unknown => None,
        nm::Band::TwoPointFour => Some("2.4 GHz".to_owned()),
        nm::Band::Five => Some("5 GHz".to_owned()),
        nm::Band::Six => Some("6 GHz".to_owned()),
    }
}

pub fn name_of(network: &Access) -> String {
    network
        .ssid
        .as_deref()
        .map(cap)
        .unwrap_or_else(|| gettext("Unnamed network"))
}

fn describe(network: &Access, metered: bool) -> String {
    let mut parts = vec![security(network.security)];
    if let Some(band) = band(network.band) {
        parts.push(band);
    }
    if network.active {
        parts.insert(0, gettext("Connected"));
        if metered {
            parts.push(gettext("Metered"));
        }
    }
    parts.join(" · ")
}

pub fn entries(
    state: &NetworkState,
    cap_at: usize,
    expanded: bool,
) -> (Vec<Entry>, Option<String>) {
    let mut rows: Vec<Entry> = Vec::new();

    let shown = match expanded || cap_at == 0 {
        true => state.networks.len(),
        false => cap_at.min(state.networks.len()),
    };
    for network in state.networks.iter().take(shown) {
        rows.push(Entry {
            id: network.id.as_str().to_owned(),
            title: name_of(network),
            subtitle: describe(network, state.metered.marked()),
            icon: network.icon_name().to_owned(),
            place: Place::Networks,
            secured: network.security.needs_a_secret(),
            selected: network.active,
            busy: network.busy.is_some(),
        });
    }

    for saved in state.known.iter().filter(|saved| !saved.in_range) {
        rows.push(Entry {
            id: saved.id.as_str().to_owned(),
            title: saved
                .name
                .clone()
                .unwrap_or_else(|| gettext("Saved network")),
            subtitle: gettext("Not in range"),
            icon: "network-wireless-offline-symbolic".to_owned(),
            place: Place::Known,
            secured: false,
            selected: false,
            busy: saved.busy.is_some(),
        });
    }

    for wired in &state.wired {
        rows.push(Entry {
            id: wired.id.as_str().to_owned(),
            title: cap(&wired.name),
            subtitle: match (wired.carrier, wired.active && state.metered.marked()) {
                (true, true) => [gettext("Connected"), gettext("Metered")].join(" · "),
                (true, false) => gettext("Connected"),
                (false, _) => gettext("Cable unplugged"),
            },
            icon: match wired.carrier {
                true => "network-wired-symbolic".to_owned(),
                false => "network-wired-disconnected-symbolic".to_owned(),
            },
            place: Place::Wired,
            secured: false,
            selected: wired.active,
            busy: wired.busy.is_some(),
        });
    }

    for vpn in &state.vpn {
        rows.push(Entry {
            id: vpn.id.as_str().to_owned(),
            title: cap(&vpn.name),
            subtitle: vpn.kind.clone(),
            icon: match vpn.active {
                true => "network-vpn-symbolic".to_owned(),
                false => "network-vpn-disconnected-symbolic".to_owned(),
            },
            place: Place::Vpn,
            secured: false,
            selected: vpn.active,
            busy: vpn.busy.is_some(),
        });
    }

    (rows, more(state.networks.len(), cap_at, expanded))
}

fn more(total: usize, cap: usize, expanded: bool) -> Option<String> {
    let hidden = match cap {
        0 => 0,
        cap => total.saturating_sub(cap),
    };
    if hidden == 0 {
        return None;
    }
    Some(match expanded {
        true => gettext("Show fewer"),
        false => ngettext(
            "{count} more network",
            "{count} more networks",
            hidden as u32,
        )
        .replace("{count}", &hidden.to_string()),
    })
}

pub fn details(state: &NetworkState, id: &str) -> Option<Details> {
    if let Some(network) = state.networks.iter().find(|one| one.id.as_str() == id) {
        let mut lines = Vec::new();
        match network.active {
            true => lines.push(Line {
                action: "disconnect".to_owned(),
                title: gettext("Disconnect"),
                activates: true,
                busy: matches!(network.busy, Some(NetworkBusy::Disconnecting)),
                ..Default::default()
            }),
            false => lines.push(Line {
                action: "connect".to_owned(),
                title: gettext("Connect"),
                activates: true,
                busy: matches!(network.busy, Some(NetworkBusy::Connecting)),
                ..Default::default()
            }),
        }
        lines.push(Line {
            action: "security".to_owned(),
            title: gettext("Security"),
            value: security(network.security),
            ..Default::default()
        });
        lines.push(Line {
            action: "signal".to_owned(),
            title: gettext("Signal"),
            value: format!("{}%", network.strength),
            ..Default::default()
        });
        if let Some(address) = &network.address {
            lines.push(Line {
                action: "address".to_owned(),
                title: gettext("IP address"),
                value: address.clone(),
                ..Default::default()
            });
        }
        if let Some(saved) = &network.saved {
            let profile = state.known.iter().find(|one| &one.id == saved);
            lines.push(Line {
                action: "autoconnect".to_owned(),
                title: gettext("Connect automatically"),
                toggle: Some(profile.is_some_and(|one| one.autoconnect)),
                ..Default::default()
            });
            lines.push(Line {
                action: "forget".to_owned(),
                title: gettext("Forget this network"),
                activates: true,
                busy: profile.is_some_and(|one| matches!(one.busy, Some(NetworkBusy::Forgetting))),
                ..Default::default()
            });
        }
        return Some(Details {
            id: id.to_owned(),
            lines,
        });
    }

    if let Some(saved) = state.known.iter().find(|one| one.id.as_str() == id) {
        return Some(Details {
            id: id.to_owned(),
            lines: vec![
                Line {
                    action: "autoconnect".to_owned(),
                    title: gettext("Connect automatically"),
                    toggle: Some(saved.autoconnect),
                    ..Default::default()
                },
                Line {
                    action: "forget".to_owned(),
                    title: gettext("Forget this network"),
                    activates: true,
                    busy: matches!(saved.busy, Some(NetworkBusy::Forgetting)),
                    ..Default::default()
                },
            ],
        });
    }

    if let Some(wired) = state.wired.iter().find(|one| one.id.as_str() == id) {
        let mut lines = vec![Line {
            action: match wired.active {
                true => "disconnect".to_owned(),
                false => "connect".to_owned(),
            },
            title: match wired.active {
                true => gettext("Disconnect"),
                false => gettext("Connect"),
            },
            activates: wired.carrier,
            busy: wired.busy.is_some(),
            ..Default::default()
        }];
        if let Some(speed) = wired.speed.filter(|speed| *speed > 0) {
            lines.push(Line {
                action: "speed".to_owned(),
                title: gettext("Speed"),
                value: gettext("{speed} Mb/s").replace("{speed}", &speed.to_string()),
                ..Default::default()
            });
        }
        if let Some(address) = &wired.address {
            lines.push(Line {
                action: "address".to_owned(),
                title: gettext("IP address"),
                value: address.clone(),
                ..Default::default()
            });
        }
        return Some(Details {
            id: id.to_owned(),
            lines,
        });
    }

    if let Some(vpn) = state.vpn.iter().find(|one| one.id.as_str() == id) {
        let mut lines = vec![Line {
            action: match vpn.active {
                true => "disconnect-vpn".to_owned(),
                false => "connect-vpn".to_owned(),
            },
            title: match vpn.active {
                true => gettext("Disconnect"),
                false => gettext("Connect"),
            },
            activates: true,
            busy: vpn.busy.is_some(),
            ..Default::default()
        }];
        if let Some(address) = &vpn.address {
            lines.push(Line {
                action: "address".to_owned(),
                title: gettext("IP address"),
                value: address.clone(),
                ..Default::default()
            });
        }
        return Some(Details {
            id: id.to_owned(),
            lines,
        });
    }
    None
}

#[cfg(test)]
mod tests {
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
            .lines
            .iter()
            .find(|line| line.action == "address")
            .expect("an address line");
        assert_eq!(address.value, "192.168.50.27/24");

        let other = state.networks[1].id.as_str().to_owned();
        let card = details(&state, &other).expect("a detail card");
        assert!(!card.lines.iter().any(|line| line.action == "address"));
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
        let actions: Vec<&str> = card.lines.iter().map(|line| line.action.as_str()).collect();
        assert_eq!(actions, ["disconnect-vpn", "address"]);

        state.vpn[0].active = false;
        state.vpn[0].address = None;
        let card = details(&state, "/s/vpn").expect("a detail card");
        assert_eq!(
            card.lines
                .iter()
                .map(|line| line.action.as_str())
                .collect::<Vec<_>>(),
            ["connect-vpn"],
            "an address the tunnel does not have is absent, not blank"
        );
    }

    #[test]
    fn a_wired_row_has_a_card_that_connects_disconnects_and_names_its_address() {
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
        let actions: Vec<&str> = card.lines.iter().map(|line| line.action.as_str()).collect();
        assert_eq!(actions, ["disconnect", "speed", "address"]);

        state.wired[0].active = false;
        state.wired[0].address = None;
        let card =
            details(&state, "/org/freedesktop/NetworkManager/Devices/459").expect("a detail card");
        assert_eq!(card.lines[0].action, "connect");

        state.wired[0].carrier = false;
        let card =
            details(&state, "/org/freedesktop/NetworkManager/Devices/459").expect("a detail card");
        assert!(
            !card.lines[0].activates,
            "an unplugged cable has nothing to connect to"
        );
    }

    #[test]
    fn the_popover_marks_a_metered_connection_whatever_the_tooltip_is_told_to_do() {
        let state = NetworkState {
            metered: nm::Metered::GuessYes,
            ..connected(70)
        };

        let (rows, _) = entries(&state, 0, true);
        let row = rows
            .iter()
            .find(|row| row.selected)
            .expect("the connected row");
        assert!(
            row.subtitle.contains(&gettext("Metered")),
            "the tooltip setting is about the bar; the row inside the popover says so regardless"
        );

        let unmetered = NetworkState {
            metered: nm::Metered::GuessNo,
            ..connected(70)
        };
        let (rows, _) = entries(&unmetered, 0, true);
        assert!(
            !rows
                .iter()
                .any(|row| row.subtitle.contains(&gettext("Metered")))
        );
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
        let (rows, more) = entries(&listed(&["a", "b", "c", "d", "e"]), 0, false);
        assert_eq!(rows.len(), 5);
        assert_eq!(
            more, None,
            "nothing is behind a row that would show nothing new"
        );
    }

    #[test]
    fn the_overflow_row_goes_both_ways() {
        let state = listed(&["a", "b", "c", "d", "e"]);

        let (rows, more) = entries(&state, 3, false);
        assert_eq!(rows.len(), 3, "the cap is what the list shows");
        assert_eq!(
            more,
            Some(
                ngettext("{count} more network", "{count} more networks", 2)
                    .replace("{count}", "2")
            )
        );

        let (rows, more) = entries(&state, 3, true);
        assert_eq!(rows.len(), 5, "expanded shows every network");
        assert_eq!(
            more,
            Some(gettext("Show fewer")),
            "a list that only ever expands strands the user at the bottom of it"
        );

        let (rows, more) = entries(&listed(&["a", "b"]), 3, false);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            more, None,
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
            ["connect", "security", "signal", "autoconnect", "forget"]
        );
        assert_eq!(
            saved.lines[3].toggle,
            Some(true),
            "the toggle reads the profile, not the access point"
        );

        state.networks[0].saved = None;
        let fresh = details(&state, "/ap/0").expect("the network");
        let actions: Vec<&str> = fresh
            .lines
            .iter()
            .map(|line| line.action.as_str())
            .collect();
        assert_eq!(
            actions,
            ["connect", "security", "signal"],
            "a network nothing is saved for has nothing to forget"
        );
    }
}
