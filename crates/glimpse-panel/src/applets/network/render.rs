use gettextrs::{gettext, ngettext};

use glimpse_dbus::network_manager as nm;
use glimpse_services::{Access, NetworkBusy, NetworkFailure, NetworkState};
use glimpse_widgets::{
    Fact, NetworkDetails as Details, NetworkEntry as Entry, NetworkLine as Line,
    NetworkPlace as Place,
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
        rows.push(access_entry(network, Place::Wifi));
    }

    let open = known.is_empty() != toggled;
    let shown = match all || cap_at == 0 {
        true => other.len(),
        false => cap_at.min(other.len()),
    };
    for network in other.iter().take(shown).copied() {
        rows.push(access_entry(network, Place::Other));
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

fn access_entry(network: &Access, place: Place) -> Entry {
    Entry {
        id: network.id.as_str().to_owned(),
        title: name_of(network),
        icon: network.icon_name().to_owned(),
        place,
        secured: network.security.needs_a_secret(),
        selected: network.active,
        busy: network.busy.is_some(),
    }
}

/// The card under a row: first the facts that describe the connection — its status, security,
/// band, speed, kind and address — then what can be done with it, how it joins before Disconnect
/// and Forget.
pub fn details(state: &NetworkState, id: &str) -> Option<Details> {
    let status = |value: String| Fact::new(gettext("Status"), value);
    let metered =
        |active: bool| (active && state.metered.marked()).then(|| status(gettext("Metered")));
    let address = |address: &Option<String>| {
        address
            .as_ref()
            .map(|address| Fact::new(gettext("IP address"), address.clone()))
    };
    let disconnect = |busy: bool, destructive: bool| Line {
        action: "disconnect".to_owned(),
        title: gettext("Disconnect"),
        activates: true,
        busy,
        destructive,
        ..Default::default()
    };
    let card = |facts: Vec<Fact>, lines: Vec<Line>| {
        (!facts.is_empty() || !lines.is_empty()).then(|| Details {
            id: id.to_owned(),
            facts,
            lines,
        })
    };

    if let Some(network) = state.networks.iter().find(|one| one.id.as_str() == id) {
        let mut facts: Vec<Fact> = metered(network.active).into_iter().collect();
        facts.push(Fact::new(gettext("Security"), security(network.security)));
        facts.extend(band(network.band).map(|band| Fact::new(gettext("Band"), band)));
        if network.active {
            facts.extend(address(&network.address));
        }
        let profile = network
            .saved
            .as_ref()
            .map(|saved| state.known.iter().find(|one| &one.id == saved));
        let mut lines = Vec::new();
        if let Some(profile) = profile {
            lines.push(Line {
                action: "autoconnect".to_owned(),
                title: gettext("Connect automatically"),
                toggle: Some(profile.is_some_and(|one| one.autoconnect)),
                ..Default::default()
            });
        }
        if network.active {
            lines.push(disconnect(
                matches!(network.busy, Some(NetworkBusy::Disconnecting)),
                profile.is_none(),
            ));
        }
        if let Some(profile) = profile {
            lines.push(Line {
                action: "forget".to_owned(),
                title: gettext("Forget this network"),
                activates: true,
                destructive: true,
                busy: profile.is_some_and(|one| matches!(one.busy, Some(NetworkBusy::Forgetting))),
                ..Default::default()
            });
        }
        return card(facts, lines);
    }

    if let Some(wired) = state.wired.iter().find(|one| one.id.as_str() == id) {
        if !wired.carrier {
            return card(vec![status(gettext("Cable unplugged"))], Vec::new());
        }
        if !wired.active {
            return None;
        }
        let mut facts: Vec<Fact> = metered(true).into_iter().collect();
        if let Some(speed) = wired.speed.filter(|speed| *speed > 0) {
            facts.push(Fact::new(
                gettext("Speed"),
                gettext("{speed} Mb/s").replace("{speed}", &speed.to_string()),
            ));
        }
        facts.extend(address(&wired.address));
        return card(facts, vec![disconnect(wired.busy.is_some(), true)]);
    }

    if let Some(vpn) = state.vpn.iter().find(|one| one.id.as_str() == id) {
        let mut facts: Vec<Fact> = (!vpn.kind.is_empty())
            .then(|| Fact::new(gettext("Type"), vpn.kind.clone()))
            .into_iter()
            .collect();
        if !vpn.active {
            return card(facts, Vec::new());
        }
        facts.extend(address(&vpn.address));
        let disconnect = Line {
            action: "disconnect-vpn".to_owned(),
            ..disconnect(vpn.busy.is_some(), true)
        };
        return card(facts, vec![disconnect]);
    }
    None
}

#[cfg(test)]
mod tests;
