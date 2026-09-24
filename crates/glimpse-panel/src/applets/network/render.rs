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

/// Other networks as the popover draws its disclosure: how many there are, whether the list is
/// open, and the wording for the row an open list ends in when the cap cut it short.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Others {
    pub count: usize,
    pub open: bool,
    pub rest: Option<String>,
}

/// Ethernet, VPN, then Wi-Fi: the network in use and every saved one in range, then the rest as
/// Other networks. The rest are closed while a known network is in range, because that is where a
/// person is and they rarely want a stranger's; with nothing known in range they open by
/// themselves. `toggled` is the header pressed against that default, `all` lifts the cap an open
/// list starts at. A saved network out of range is not listed.
pub fn entries(
    state: &NetworkState,
    cap_at: usize,
    toggled: bool,
    all: bool,
) -> (Vec<Entry>, Others) {
    let mut rows: Vec<Entry> = Vec::new();

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

    let (mut known, other): (Vec<&Access>, Vec<&Access>) = state
        .networks
        .iter()
        .partition(|network| network.active || network.saved.is_some());
    known.sort_by_key(|network| !network.active);
    for network in known.iter().copied() {
        rows.push(access_entry(state, network, Place::Wifi));
    }

    let open = known.is_empty() != toggled;
    let shown = match all || cap_at == 0 {
        true => other.len(),
        false => cap_at.min(other.len()),
    };
    for network in other.iter().take(shown).copied() {
        rows.push(access_entry(state, network, Place::Other));
    }

    let hidden = other.len() - shown;
    let others = Others {
        count: other.len(),
        open,
        rest: (hidden > 0).then(|| {
            ngettext(
                "{count} more network",
                "{count} more networks",
                hidden as u32,
            )
            .replace("{count}", &hidden.to_string())
        }),
    };
    (rows, others)
}

fn access_entry(state: &NetworkState, network: &Access, place: Place) -> Entry {
    let subtitle = match place {
        Place::Other => stranger(network.security),
        _ => describe(network, state.metered.marked()),
    };
    Entry {
        id: network.id.as_str().to_owned(),
        title: name_of(network),
        subtitle,
        icon: network.icon_name().to_owned(),
        place,
        secured: network.security.needs_a_secret(),
        selected: network.active,
        busy: network.busy.is_some(),
    }
}

/// A stranger is one line: the padlock already says it is secured and the band helps nobody
/// choose. Only what changes the decision to join is said — no encryption at all, or a network
/// that asks for more than a password.
fn stranger(security: nm::Security) -> String {
    match security {
        nm::Security::Open | nm::Security::Enterprise => self::security(security),
        _ => String::new(),
    }
}

/// The card under a row, or `None` for a row that has nothing to manage. A connection in use
/// carries Disconnect and what describes it; a saved network carries how it joins and Forget. A
/// stranger, and a wired or VPN connection not in use, has no card: its row's body is its one
/// action, and Security and Signal would only repeat the subtitle and the icon.
pub fn details(state: &NetworkState, id: &str) -> Option<Details> {
    let disconnect = |busy: bool, destructive: bool| Line {
        action: "disconnect".to_owned(),
        title: gettext("Disconnect"),
        activates: true,
        busy,
        destructive,
        ..Default::default()
    };
    let address = |address: &Option<String>| {
        address.as_ref().map(|address| Line {
            action: "address".to_owned(),
            title: gettext("IP address"),
            value: address.clone(),
            ..Default::default()
        })
    };
    let card = |lines: Vec<Line>| {
        (!lines.is_empty()).then(|| Details {
            id: id.to_owned(),
            lines,
        })
    };

    if let Some(network) = state.networks.iter().find(|one| one.id.as_str() == id) {
        let mut lines = Vec::new();
        if network.active {
            lines.push(disconnect(
                matches!(network.busy, Some(NetworkBusy::Disconnecting)),
                network.saved.is_none(),
            ));
            lines.extend(address(&network.address));
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
                destructive: true,
                busy: profile.is_some_and(|one| matches!(one.busy, Some(NetworkBusy::Forgetting))),
                ..Default::default()
            });
        }
        return card(lines);
    }

    if let Some(wired) = state.wired.iter().find(|one| one.id.as_str() == id) {
        if !wired.active {
            return None;
        }
        let mut lines = vec![disconnect(wired.busy.is_some(), true)];
        if let Some(speed) = wired.speed.filter(|speed| *speed > 0) {
            lines.push(Line {
                action: "speed".to_owned(),
                title: gettext("Speed"),
                value: gettext("{speed} Mb/s").replace("{speed}", &speed.to_string()),
                ..Default::default()
            });
        }
        lines.extend(address(&wired.address));
        return card(lines);
    }

    if let Some(vpn) = state.vpn.iter().find(|one| one.id.as_str() == id) {
        if !vpn.active {
            return None;
        }
        let mut lines = vec![Line {
            action: "disconnect-vpn".to_owned(),
            ..disconnect(vpn.busy.is_some(), true)
        }];
        lines.extend(address(&vpn.address));
        return card(lines);
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
        assert!(
            details(&state, &other).is_none(),
            "a stranger has nothing to manage, so its row joins and carries no card"
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
        let actions: Vec<&str> = card.lines.iter().map(|line| line.action.as_str()).collect();
        assert_eq!(actions, ["disconnect-vpn", "address"]);
        assert!(
            card.lines[0].destructive,
            "a lone Disconnect is the card's red action"
        );

        state.vpn[0].active = false;
        state.vpn[0].address = None;
        assert!(
            details(&state, "/s/vpn").is_none(),
            "a tunnel not in use connects from its row and has no card"
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
        let actions: Vec<&str> = card.lines.iter().map(|line| line.action.as_str()).collect();
        assert_eq!(actions, ["disconnect", "speed", "address"]);

        state.wired[0].active = false;
        state.wired[0].address = None;
        assert!(
            details(&state, "/org/freedesktop/NetworkManager/Devices/459").is_none(),
            "a cable not in use connects from its row and has no card"
        );
    }

    #[test]
    fn the_popover_marks_a_metered_connection_whatever_the_tooltip_is_told_to_do() {
        let state = NetworkState {
            metered: nm::Metered::GuessYes,
            ..connected(70)
        };

        let (rows, _) = entries(&state, 0, false, false);
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
        let (rows, _) = entries(&unmetered, 0, false, false);
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
            Some(
                ngettext("{count} more network", "{count} more networks", 2)
                    .replace("{count}", "2")
            )
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
            ["autoconnect", "forget"],
            "the row joins; the card is only what a profile has, and nothing the row already says"
        );
        assert_eq!(
            saved.lines[0].toggle,
            Some(true),
            "the toggle reads the profile, not the access point"
        );

        state.networks[0].saved = None;
        assert!(
            details(&state, "/ap/0").is_none(),
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
        assert!(
            strangers.iter().all(|row| row.subtitle.is_empty()),
            "a secured stranger is one line: the padlock says secured, the band helps nobody"
        );
    }
}
