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

fn reading(device: &Device) -> String {
    let Some(battery) = device.battery else {
        return gettext("Connected");
    };
    match (battery.charge, battery.charging) {
        (Some(charge), true) => {
            gettext("{percent}, charging").replace("{percent}", &percent(charge))
        }
        (Some(charge), false) => percent(charge),
        (None, _) => gettext("Connected"),
    }
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

pub fn summary(state: &KdeconnectState) -> String {
    let connected = paired(state).filter(|device| device.reachable).count();
    if connected > 0 {
        return ngettext("{count} connected", "{count} connected", connected as u32)
            .replace("{count}", &connected.to_string());
    }
    match paired(state).next() {
        Some(_) => gettext("Not connected"),
        None => gettext("No paired devices"),
    }
}

pub fn any_paired(state: &KdeconnectState) -> bool {
    paired(state).next().is_some()
}

fn row(key: &str, label: String) -> Row {
    Row {
        key: key.to_owned(),
        label,
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
                subtitle: String::new(),
                value: match device.reachable {
                    true => reading(device),
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
mod tests {
    use glimpse_services::{
        KdeconnectActions as Actions, KdeconnectBattery as Battery, KdeconnectDeviceId,
    };

    use super::*;

    fn device(id: &str, reachable: bool, pair: PairState) -> Device {
        let connected = reachable && pair == PairState::Paired;
        Device {
            id: KdeconnectDeviceId::new(id),
            name: format!("Pixel {id}"),
            kind: DeviceType::Phone,
            reachable,
            pair,
            battery: connected.then_some(Battery {
                charge: Some(76),
                charging: false,
                low: false,
            }),
            actions: match connected {
                true => Actions {
                    ring: true,
                    ping: true,
                    send_clipboard: false,
                    share: true,
                    browse: true,
                    messages: true,
                },
                false => Actions::default(),
            },
        }
    }

    fn state(devices: Vec<Device>) -> KdeconnectState {
        KdeconnectState {
            running: true,
            devices,
        }
    }

    fn config() -> KdeconnectAppletConfig {
        KdeconnectAppletConfig::default()
    }

    #[test]
    fn no_daemon_is_no_chip() {
        assert_eq!(chip(&KdeconnectState::default(), &config()), None);
    }

    #[test]
    fn a_running_daemon_with_nothing_paired_is_a_plain_chip() {
        let chip = chip(&state(vec![]), &config()).expect("a chip");
        assert_eq!(chip.icon, Some(ICON));
        assert_eq!(chip.label, None);
        assert_eq!(chip.tooltip, "No paired devices");
    }

    #[test]
    fn a_connected_phone_shows_its_battery_by_default() {
        let chip = chip(
            &state(vec![device("10", true, PairState::Paired)]),
            &config(),
        )
        .expect("a chip");
        assert_eq!(chip.icon, Some(ICON));
        assert_eq!(chip.label.as_deref(), Some("76%"));
        assert_eq!(chip.overlay, None);
        assert_eq!(chip.tooltip, "Pixel 10 · 76%");
    }

    #[test]
    fn no_battery_is_no_label_never_a_bare_percent() {
        let mut phone = device("10", true, PairState::Paired);
        phone.battery = None;
        let chip = chip(&state(vec![phone]), &config()).expect("a chip");
        assert_eq!(chip.label, None);
        assert_eq!(chip.tooltip, "Pixel 10 · Connected");
    }

    #[test]
    fn every_style_and_token_renders_as_designed() {
        let mut phone = device("10", true, PairState::Paired);
        phone.battery = Some(Battery {
            charge: Some(76),
            charging: true,
            low: false,
        });
        let devices = state(vec![phone]);
        let render = |style, format: &str| {
            let chip = chip(
                &devices,
                &KdeconnectAppletConfig {
                    indicator_style: style,
                    label_format: format.to_owned(),
                    ..config()
                },
            )
            .expect("a chip");
            (chip.icon, chip.label)
        };
        assert_eq!(
            render(BatteryIndicatorStyle::IconOnly, "{battery}"),
            (Some(ICON), None)
        );
        assert_eq!(
            render(BatteryIndicatorStyle::IconText, "{battery} {charging}"),
            (Some(ICON), Some("76% charging".to_owned()))
        );
        assert_eq!(
            render(BatteryIndicatorStyle::Text, "{name}"),
            (None, Some("Pixel 10".to_owned()))
        );
        assert_eq!(
            render(BatteryIndicatorStyle::Text, "{charging}{nonesuch}"),
            (None, Some("charging{nonesuch}".to_owned()))
        );
    }

    #[test]
    fn text_style_with_nothing_to_say_falls_back_to_the_icon() {
        let mut phone = device("10", true, PairState::Paired);
        phone.battery = None;
        let chip = chip(
            &state(vec![phone]),
            &KdeconnectAppletConfig {
                indicator_style: BatteryIndicatorStyle::Text,
                ..config()
            },
        )
        .expect("a chip");
        assert_eq!((chip.icon, chip.label), (Some(ICON), None));
    }

    #[test]
    fn an_away_phone_carries_the_offline_emblem_unless_hidden() {
        let away = state(vec![device("8", false, PairState::Paired)]);
        let chip = chip(&away, &config()).expect("a chip");
        assert_eq!(chip.overlay, Some(OFFLINE));
        assert_eq!(chip.label, None);
        assert_eq!(chip.tooltip, "Pixel 8 · Not connected");

        let hidden = KdeconnectAppletConfig {
            hide_when_disconnected: true,
            ..config()
        };
        assert_eq!(super::chip(&away, &hidden), None);
        assert_eq!(
            super::chip(&state(vec![]), &hidden),
            None,
            "nothing paired is nothing connected"
        );
    }

    #[test]
    fn a_pair_request_is_a_notice_even_when_the_chip_would_hide() {
        let asking = state(vec![
            device("8", false, PairState::Paired),
            device("tab", true, PairState::RequestedByPeer),
        ]);
        let hidden = KdeconnectAppletConfig {
            hide_when_disconnected: true,
            ..config()
        };
        let chip = chip(&asking, &hidden).expect("a request keeps the chip");
        assert!(chip.notice);
        assert!(
            chip.tooltip.ends_with("Pixel tab wants to pair"),
            "{}",
            chip.tooltip
        );
    }

    #[test]
    fn low_battery_comes_through_from_the_service() {
        let mut phone = device("10", true, PairState::Paired);
        phone.battery = Some(Battery {
            charge: Some(9),
            charging: false,
            low: true,
        });
        assert!(chip(&state(vec![phone]), &config()).expect("a chip").low);
    }

    #[test]
    fn the_chip_follows_the_configured_device_then_the_first_connected() {
        let both = state(vec![
            device("8", false, PairState::Paired),
            device("10", true, PairState::Paired),
        ]);
        assert_eq!(
            followed(&both, None).map(|device| device.name.as_str()),
            Some("Pixel 10")
        );
        assert_eq!(
            followed(&both, Some("Pixel 8")).map(|device| device.name.as_str()),
            Some("Pixel 8")
        );
        assert_eq!(followed(&both, Some("Nokia")), None);
    }

    #[test]
    fn a_hostile_name_is_cleaned_and_capped_everywhere_it_lands() {
        let mut phone = device("10", true, PairState::Paired);
        phone.name = format!("<b>{}</b>\n\u{202e}", "x".repeat(4096));
        let devices = state(vec![phone]);
        let rows = super::devices(&devices);
        assert_eq!(rows[0].title.chars().count(), NAME_CAP + 1);
        assert!(!rows[0].title.contains('\n'));
        assert!(!rows[0].title.contains('\u{202e}'));
        let chip = chip(
            &devices,
            &KdeconnectAppletConfig {
                label_format: "{name}".to_owned(),
                ..config()
            },
        )
        .expect("a chip");
        assert!(chip.label.expect("a label").chars().count() <= LABEL_CAP + 1);
    }

    #[test]
    fn a_connected_device_offers_its_actions_and_unpair_last() {
        let rows = devices(&state(vec![device("10", true, PairState::Paired)]));
        let keys: Vec<&str> = rows[0].actions.iter().map(|row| row.key.as_str()).collect();
        assert_eq!(keys, [RING, PING, SHARE, BROWSE, MESSAGES, UNPAIR]);
        assert_eq!(rows[0].subtitle, "", "a device row is one line");
        assert_eq!(rows[0].value, "76%");
    }

    #[test]
    fn an_away_device_offers_only_unpair_and_says_so_in_its_value() {
        let rows = devices(&state(vec![device("8", false, PairState::Paired)]));
        let keys: Vec<&str> = rows[0].actions.iter().map(|row| row.key.as_str()).collect();
        assert_eq!(keys, [UNPAIR]);
        assert_eq!(rows[0].value, "Not connected");
    }

    #[test]
    fn connected_devices_list_first() {
        let rows = devices(&state(vec![
            device("8", false, PairState::Paired),
            device("10", true, PairState::Paired),
        ]));
        let titles: Vec<&str> = rows.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(titles, ["Pixel 10", "Pixel 8"]);
    }

    #[test]
    fn send_clipboard_appears_only_when_the_daemon_is_not_syncing_already() {
        let mut phone = device("10", true, PairState::Paired);
        phone.actions.send_clipboard = true;
        let rows = devices(&state(vec![phone]));
        assert!(rows[0].actions.iter().any(|row| row.key == CLIPBOARD));
    }

    #[test]
    fn nearby_is_every_unpaired_device_capped_with_a_count() {
        let mut found: Vec<Device> = (0..8)
            .map(|at| device(&format!("n{at}"), true, PairState::NotPaired))
            .collect();
        found[0].pair = PairState::Requested;
        found[1].pair = PairState::RequestedByPeer;
        found.push(device("10", true, PairState::Paired));
        let (rows, more) = nearby(&state(found));
        assert_eq!(rows.len(), NEARBY_CAP);
        assert_eq!(more.as_deref(), Some("3 more"));
        assert!(rows[0].busy);
        assert_eq!(rows[1].subtitle, "Wants to pair");
        assert!(rows.iter().all(|row| row.title != "Pixel 10"));
    }

    #[test]
    fn the_summary_counts_connected_devices() {
        assert_eq!(summary(&state(vec![])), "No paired devices");
        assert_eq!(
            summary(&state(vec![device("8", false, PairState::Paired)])),
            "Not connected"
        );
        assert_eq!(
            summary(&state(vec![
                device("8", true, PairState::Paired),
                device("10", true, PairState::Paired),
            ])),
            "2 connected"
        );
    }

    #[test]
    fn only_known_keys_map_to_a_service_action() {
        assert_eq!(action(RING), Some(KdeconnectAction::Ring));
        assert_eq!(action(UNPAIR), Some(KdeconnectAction::Unpair));
        assert_eq!(action(SHARE), None);
        assert_eq!(action("nonesuch"), None);
    }
}
