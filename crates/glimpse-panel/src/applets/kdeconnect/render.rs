use gettextrs::{gettext, ngettext};
use glimpse_config::{BatteryIndicatorStyle, KdeconnectAppletConfig};
use glimpse_services::{
    KdeconnectAction, KdeconnectDevice as Device, KdeconnectDeviceType as DeviceType,
    KdeconnectPairState as PairState, KdeconnectState,
};
use glimpse_widgets::{KdeconnectAction as Row, KdeconnectDevice, KdeconnectNearby};

pub const ICON: &str = "phone-symbolic";
const OFFLINE: &str = "network-offline-symbolic";
const NAME_CAP: usize = 24;
const LABEL_CAP: usize = 24;
const TOOLTIP_CAP: usize = 120;
const NEARBY_CAP: usize = 5;
const TOOLTIP_LINES: usize = 6;

pub const RING: &str = "ring";
pub const PING: &str = "ping";
pub const CLIPBOARD: &str = "clipboard";
pub const SHARE: &str = "share";
pub const BROWSE: &str = "browse";
pub const MESSAGES: &str = "messages";
pub const UNPAIR: &str = "unpair";

pub fn action(key: &str) -> Option<KdeconnectAction> {
    match key {
        RING => Some(KdeconnectAction::Ring),
        PING => Some(KdeconnectAction::Ping),
        CLIPBOARD => Some(KdeconnectAction::SendClipboard),
        BROWSE => Some(KdeconnectAction::Browse),
        MESSAGES => Some(KdeconnectAction::OpenMessages),
        UNPAIR => Some(KdeconnectAction::Unpair),
        _ => None,
    }
}

fn name(device: &Device) -> String {
    glimpse_utils::clean(&device.name, NAME_CAP)
}

pub fn icon(kind: DeviceType) -> &'static str {
    match kind {
        DeviceType::Tablet => "tablet-symbolic",
        DeviceType::Tv => "tv-symbolic",
        DeviceType::Desktop | DeviceType::Laptop => "computer-symbolic",
        DeviceType::Phone | DeviceType::Unknown => ICON,
    }
}

fn paired(state: &KdeconnectState) -> impl Iterator<Item = &Device> {
    state
        .devices
        .iter()
        .filter(|device| device.pair == PairState::Paired)
}

fn connected(device: &Device) -> bool {
    device.reachable && device.pair == PairState::Paired
}

pub fn followed<'a>(state: &'a KdeconnectState, wanted: Option<&str>) -> Option<&'a Device> {
    if let Some(wanted) = wanted {
        return paired(state).find(|device| device.name == wanted);
    }
    paired(state)
        .find(|device| device.reachable)
        .or_else(|| paired(state).next())
}

fn percent(charge: u8) -> String {
    gettext("{percent}%").replace("{percent}", &charge.to_string())
}

fn charge(device: &Device) -> Option<String> {
    let battery = device.battery?;
    let charge = percent(battery.charge?);
    Some(match battery.charging {
        true => gettext("{percent}, charging").replace("{percent}", &charge),
        false => charge,
    })
}

fn reading(device: &Device) -> String {
    charge(device).unwrap_or_else(|| gettext("Connected"))
}

fn line(device: &Device) -> String {
    let state = match (device.pair, connected(device)) {
        (PairState::RequestedByPeer, _) => {
            return gettext("{name} wants to pair").replace("{name}", &name(device));
        }
        (_, true) => reading(device),
        (_, false) => gettext("Not connected"),
    };
    gettext("{name} · {state}")
        .replace("{name}", &name(device))
        .replace("{state}", &state)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub icon: Option<&'static str>,
    pub overlay: Option<&'static str>,
    pub label: Option<String>,
    pub tooltip: String,
    pub low: bool,
    pub notice: bool,
}

pub fn tooltip(state: &KdeconnectState, config: &KdeconnectAppletConfig, format: &str) -> String {
    match followed(state, config.device.as_deref()) {
        Some(device) => formatted(device, format, TOOLTIP_CAP),
        None => gettext("No paired devices"),
    }
}

fn label(device: &Device, format: &str) -> String {
    formatted(device, format, LABEL_CAP)
}

fn formatted(device: &Device, format: &str, cap: usize) -> String {
    let name = name(device);
    let battery = device
        .battery
        .and_then(|battery| battery.charge)
        .map(percent)
        .unwrap_or_default();
    let charging = match device.battery.is_some_and(|battery| battery.charging) {
        true => gettext("charging"),
        false => String::new(),
    };
    let rendered = crate::applets::tokens::render(format, |token| match token {
        "name" => Some(name.as_str()),
        "battery" => Some(battery.as_str()),
        "charging" => Some(charging.as_str()),
        _ => None,
    });
    glimpse_utils::clean(&rendered, cap)
}

pub fn chip(state: &KdeconnectState, config: &KdeconnectAppletConfig) -> Option<Chip> {
    if !state.running {
        return None;
    }
    let notice = state
        .devices
        .iter()
        .any(|device| device.pair == PairState::RequestedByPeer);
    let mut lines: Vec<String> = paired(state)
        .chain(
            state
                .devices
                .iter()
                .filter(|device| device.pair == PairState::RequestedByPeer),
        )
        .take(TOOLTIP_LINES)
        .map(line)
        .collect();
    if lines.is_empty() {
        lines.push(gettext("No paired devices"));
    }
    let tooltip = lines.join("\n");

    let Some(device) = followed(state, config.device.as_deref()) else {
        if config.hide_when_disconnected && !notice {
            return None;
        }
        return Some(Chip {
            icon: Some(ICON),
            overlay: None,
            label: None,
            tooltip,
            low: false,
            notice,
        });
    };
    if !device.reachable {
        if config.hide_when_disconnected && !notice {
            return None;
        }
        return Some(Chip {
            icon: Some(icon(device.kind)),
            overlay: Some(OFFLINE),
            label: None,
            tooltip,
            low: false,
            notice,
        });
    }

    let text = label(device, &config.label_format);
    let text = (!text.is_empty()).then_some(text);
    let (icon, label) = match config.indicator_style {
        BatteryIndicatorStyle::IconOnly => (Some(icon(device.kind)), None),
        BatteryIndicatorStyle::IconText => (Some(icon(device.kind)), text),
        BatteryIndicatorStyle::Text if text.is_none() => (Some(icon(device.kind)), None),
        BatteryIndicatorStyle::Text => (None, text),
    };
    Some(Chip {
        icon,
        overlay: None,
        label,
        tooltip,
        low: device.battery.is_some_and(|battery| battery.low),
        notice,
    })
}

pub fn any_paired(state: &KdeconnectState) -> bool {
    paired(state).next().is_some()
}

fn row(key: &str, label: String) -> Row {
    Row {
        key: key.to_owned(),
        label,
        destructive: key == UNPAIR,
    }
}

pub fn devices(state: &KdeconnectState) -> Vec<KdeconnectDevice> {
    let mut devices: Vec<&Device> = paired(state).collect();
    devices.sort_by_key(|device| !device.reachable);
    devices
        .into_iter()
        .map(|device| {
            let mut actions = Vec::new();
            if device.reachable {
                if device.actions.ring {
                    actions.push(row(RING, gettext("Ring")));
                }
                if device.actions.ping {
                    actions.push(row(PING, gettext("Ping")));
                }
                if device.actions.send_clipboard {
                    actions.push(row(CLIPBOARD, gettext("Send clipboard")));
                }
                if device.actions.share {
                    actions.push(row(SHARE, gettext("Send files…")));
                }
                if device.actions.browse {
                    actions.push(row(BROWSE, gettext("Browse files")));
                }
                if device.actions.messages {
                    actions.push(row(MESSAGES, gettext("Open SMS")));
                }
            }
            actions.push(row(UNPAIR, gettext("Unpair")));
            KdeconnectDevice {
                id: device.id.as_str().to_owned(),
                title: name(device),
                value: match device.reachable {
                    true => charge(device).unwrap_or_default(),
                    false => gettext("Not connected"),
                },
                actions,
            }
        })
        .collect()
}

pub fn nearby(state: &KdeconnectState) -> (Vec<KdeconnectNearby>, Option<String>) {
    let found: Vec<&Device> = state
        .devices
        .iter()
        .filter(|device| device.pair != PairState::Paired && device.reachable)
        .collect();
    let hidden = found.len().saturating_sub(NEARBY_CAP);
    let rows = found
        .into_iter()
        .take(NEARBY_CAP)
        .map(|device| KdeconnectNearby {
            id: device.id.as_str().to_owned(),
            title: name(device),
            subtitle: match device.pair {
                PairState::RequestedByPeer => gettext("Wants to pair"),
                _ => String::new(),
            },
            busy: device.pair == PairState::Requested,
        })
        .collect();
    let more = (hidden > 0).then(|| {
        ngettext("{count} more", "{count} more", hidden as u32)
            .replace("{count}", &hidden.to_string())
    });
    (rows, more)
}

#[cfg(test)]
mod tests;
