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
fn a_connected_device_without_a_battery_reading_shows_no_value() {
    let mut phone = device("10", true, PairState::Paired);
    phone.battery = None;
    assert_eq!(devices(&state(vec![phone.clone()]))[0].value, "");
    phone.battery = Some(Battery {
        charge: None,
        charging: false,
        low: false,
    });
    assert_eq!(devices(&state(vec![phone]))[0].value, "");
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
fn only_known_keys_map_to_a_service_action() {
    assert_eq!(action(RING), Some(KdeconnectAction::Ring));
    assert_eq!(action(UNPAIR), Some(KdeconnectAction::Unpair));
    assert_eq!(action(SHARE), None);
    assert_eq!(action("nonesuch"), None);
}
