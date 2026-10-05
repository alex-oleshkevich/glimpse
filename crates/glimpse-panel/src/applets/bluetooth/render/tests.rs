use glimpse_services::{Adapter, Device, DeviceId};

use super::*;

fn device(name: &str, connected: bool) -> Device {
    Device {
        id: DeviceId::new(format!("/org/bluez/hci0/dev_{name}")),
        address: "00:00:00:00:00:00".to_owned(),
        name: name.to_owned(),
        icon: glimpse_dbus::bluez::DeviceIcon::Headset,
        paired: true,
        bonded: true,
        trusted: true,
        blocked: false,
        connected,
        battery: None,
        codec: None,
        rssi: None,
        profiles: Vec::new(),
        busy: None,
        failure: None,
    }
}

fn state(power: Power, scanning: bool, devices: Vec<Device>) -> BluetoothState {
    BluetoothState {
        adapter: Some(Adapter {
            alias: "glimpse".to_owned(),
            power,
            discoverable: false,
        }),
        devices,
        scan: scanning.then_some(glimpse_services::Hold::Held),
        pairing: None,
        confirm: None,
    }
}

#[test]
fn a_machine_with_no_radio_offers_no_tooltip() {
    assert!(tooltip(&BluetoothState::default(), None).is_none());
}

#[test]
fn the_bar_carries_an_icon_and_never_a_device_name() {
    let one = state(Power::On, false, vec![device("Buds", true)]);
    assert_eq!(one.icon_name().unwrap(), "bluetooth-active-symbolic");
    assert_eq!(
        tooltip(&one, None).as_deref(),
        Some("1 device connected"),
        "the bar counts what is connected and never names it"
    );

    let many = state(
        Power::On,
        false,
        vec![device("Buds", true), device("Mouse", true)],
    );
    assert_eq!(many.icon_name().unwrap(), "bluetooth-active-symbolic");
    assert_eq!(tooltip(&many, None).as_deref(), Some("2 devices connected"));
    assert_eq!(
        tooltip(&state(Power::On, false, vec![]), None).as_deref(),
        Some("No devices connected")
    );
}

#[test]
fn a_hostile_name_is_capped_before_it_reaches_a_tooltip() {
    let name = "Наушники ".repeat(20);
    let shown = tooltip(
        &state(Power::On, false, vec![device(&name, true)]),
        Some("{devices}"),
    )
    .expect("a tooltip");

    assert_eq!(shown.chars().count(), NAME_CAP + 1);
    assert!(
        shown.ends_with('…'),
        "a cut name must not read as a whole one"
    );
}

#[test]
fn devices_are_placed_by_what_they_are_and_bounded_by_the_config() {
    let mut devices = vec![device("Buds", true)];
    for index in 0..10 {
        let mut paired = device(&format!("paired{index}"), false);
        paired.name = format!("paired{index}");
        devices.push(paired);
    }
    for index in 0..10 {
        let mut found = device(&format!("near{index}"), false);
        found.paired = false;
        found.bonded = false;
        found.trusted = false;
        devices.push(found);
    }
    let state = state(Power::On, true, devices);

    let listing = entries(&state, 6, 8, (false, false), false);
    let counted = |place: Place| counted_in(&listing, place);

    assert_eq!(counted(Place::Connected), 1);
    assert_eq!(
        counted(Place::Paired),
        6,
        "the rest go behind an overflow row, not onto a popover nothing scrolls"
    );
    assert!(listing.more_paired.is_some());
    assert_eq!(
        (
            counted(Place::Nearby),
            listing.nearby_count,
            listing.nearby_open
        ),
        (8, 10, false),
        "with devices of its own a person rarely wants a stranger's, so Nearby starts closed"
    );
    assert!(listing.more_nearby.is_some());

    let toggled = entries(&state, 6, 8, (false, false), true);
    assert_eq!(counted_in(&toggled, Place::Nearby), 8);
    assert!(toggled.nearby_open && toggled.more_nearby.is_some());

    let opened = entries(&state, 6, 8, (true, true), false);
    assert_eq!(counted_in(&opened, Place::Paired), 10);
    assert_eq!(
        opened.more_paired.as_deref(),
        Some("Show fewer"),
        "an expanded list keeps the row that collapses it, or the popover only grows"
    );
}

fn counted_in(listing: &Listing, place: Place) -> usize {
    listing
        .entries
        .iter()
        .filter(|entry| entry.place == place)
        .count()
}

#[test]
fn a_device_that_lost_its_bond_says_so_on_its_row() {
    let mut dropped = device("Pixel", false);
    dropped.failure = Some(Failure::BondBroken);
    let state = state(Power::On, false, vec![dropped]);

    let row = &entries(&state, 6, 8, (false, false), false).entries[0];

    assert_eq!(
        row.problem, "Pairing lost",
        "a bond the remote forgot leaves Connect failing forever with nothing saying why"
    );
}

#[test]
fn a_busy_device_spins_rather_than_spelling_out_what_it_is_doing() {
    for doing in [
        Busy::Connecting,
        Busy::Disconnecting,
        Busy::Pairing,
        Busy::Forgetting,
    ] {
        let mut busy = device("Buds", false);
        busy.busy = Some(doing);
        busy.battery = Some(80);
        let state = state(Power::On, false, vec![busy]);

        let row = &entries(&state, 6, 8, (false, false), false).entries[0];

        assert!(row.busy, "{doing:?} must reach the row as a spinner");
    }
}

#[test]
fn a_device_that_is_pairing_spins_on_its_own_row() {
    let mut pairing = device("Buds", false);
    pairing.paired = false;
    pairing.bonded = false;
    pairing.trusted = false;
    pairing.busy = Some(Busy::Pairing);
    let id = pairing.id.clone();
    let state = state(Power::On, true, vec![pairing]);

    let row = entries(&state, 6, 8, (false, false), false)
        .entries
        .into_iter()
        .find(|entry| entry.id == id.as_str())
        .expect("the device is listed");
    assert_eq!(row.place, Place::Nearby);
    assert!(
        row.busy,
        "pairing is started from the row, so the row is what spins"
    );
}

#[test]
fn a_details_page_offers_connect_or_disconnect_but_never_both() {
    let mut connected = device("Buds", true);
    connected.codec = Some(Codec::Ldac);
    connected.battery = Some(80);
    let id = connected.id.clone();
    let connected_state = state(Power::On, false, vec![connected]);

    let details = details(&connected_state, &id).expect("the device");
    let actions: Vec<&str> = details
        .lines
        .iter()
        .map(|line| line.action.as_str())
        .collect();
    let labels: Vec<&str> = details
        .facts
        .iter()
        .map(|fact| fact.label.as_str())
        .collect();

    assert_eq!(actions, ["disconnect", "trust", "forget"]);
    assert_eq!(labels, ["Type", "Battery", "Codec"]);
    assert_eq!(details.facts[1].value, "80%");
    let red: Vec<&str> = details
        .lines
        .iter()
        .filter(|line| line.destructive)
        .map(|line| line.action.as_str())
        .collect();
    assert_eq!(red, ["forget"], "Disconnect beside Forget stays plain");

    let state = state(Power::On, false, vec![device("Buds", false)]);
    let paired = super::details(&state, &id).expect("the paired device");
    let actions: Vec<&str> = paired
        .lines
        .iter()
        .map(|line| line.action.as_str())
        .collect();
    assert_eq!(actions, ["connect", "trust", "forget"]);
}

#[test]
fn a_nearby_device_card_offers_pair_first() {
    let mut found = device("Bose", false);
    found.paired = false;
    found.bonded = false;
    found.trusted = false;
    found.rssi = Some(-73);
    let id = found.id.clone();
    let state = state(Power::On, true, vec![found]);

    let card = details(&state, &id).expect("a card");
    assert!(
        card.lines.len() == 1 && card.facts.len() == 1,
        "a nearby device offers pairing and names its kind"
    );
    assert_eq!(card.lines[0].action, "pair");
    assert_eq!(card.lines[0].title, "Pair");
    assert!(card.lines[0].activates && !card.lines[0].destructive);
}

#[test]
fn a_codec_row_exists_only_while_connected() {
    let mut idle = device("Buds", false);
    idle.codec = Some(Codec::Ldac);
    let id = idle.id.clone();
    let state = state(Power::On, false, vec![idle]);

    let details = details(&state, &id).expect("the device");

    assert!(
        !details.facts.iter().any(|fact| fact.label == "Codec"),
        "the transport is gone the moment the device disconnects"
    );
}

#[test]
fn a_pairing_that_will_not_survive_a_restart_is_said_on_the_row() {
    let mut fragile = device("Buds", false);
    fragile.bonded = false;
    let id = fragile.id.clone();
    let mut sound = device("Keys", false);
    sound.id = DeviceId::new("/org/bluez/hci0/dev_11_22_33_44_55_66");
    let state = state(Power::On, false, vec![fragile, sound]);

    let rows = entries(&state, 6, 8, (false, false), false).entries;
    let (warned, calm): (Vec<Entry>, Vec<Entry>) =
        rows.into_iter().partition(|entry| entry.id == id.as_str());
    assert!(
        !warned[0].problem.is_empty(),
        "a problem is seen without opening anything"
    );
    assert!(calm.iter().all(|entry| entry.problem.is_empty()));
}

#[test]
fn a_failure_reaches_the_user_as_a_sentence_and_never_as_a_token() {
    let told = wording(Failure::Unreachable);

    assert!(!told.is_empty());
    assert!(!told.contains("br-connection"));
    assert!(told.ends_with('.'));
}

#[test]
fn the_hero_switch_is_dead_while_rfkill_holds_the_radio() {
    let blocked = hero(&state(Power::Blocked, false, vec![]));

    assert!(!blocked.settable, "writing Powered = true cannot succeed");
    assert!(!blocked.on);
    assert!(hero(&state(Power::On, false, vec![])).settable);
}

#[test]
fn a_tooltip_format_fills_its_placeholders() {
    let state = state(Power::On, false, vec![device("Buds", true)]);

    assert_eq!(
        tooltip(&state, Some("{adapter}: {devices}")).as_deref(),
        Some("glimpse: Buds")
    );
    assert_eq!(tooltip(&state, None).as_deref(), Some("1 device connected"));
}

#[test]
fn a_token_inside_a_device_name_is_not_substituted() {
    let state = state(Power::On, false, vec![device("{adapter}", true)]);

    assert_eq!(
        tooltip(&state, Some("{devices}")).as_deref(),
        Some("{adapter}"),
        "a device name is remote-supplied and must never be read as a template"
    );
}

fn asking(prompt: Prompt) -> BluetoothState {
    let mut state = state(Power::On, false, vec![device("Pixel", false)]);
    state.pairing = Some(prompt);
    state
}

fn pixel() -> DeviceId {
    device("Pixel", false).id
}

#[test]
fn one_rule_decides_which_prompts_become_a_page() {
    let every = [
        Prompt::Confirm {
            device: pixel(),
            passkey: 0,
        },
        Prompt::Authorize(pixel()),
        Prompt::RequestPin(pixel()),
        Prompt::RequestPasskey(pixel()),
        Prompt::DisplayPin {
            device: pixel(),
            pin: "0000".to_owned(),
        },
        Prompt::DisplayPasskey {
            device: pixel(),
            passkey: 0,
            entered: 0,
        },
    ];

    for one in every {
        let is_typed = typed(&one).is_some();
        let named = format!("{one:?}");
        assert_eq!(
            prompt(&asking(one)).is_none(),
            is_typed,
            "the watch and the page disagree about who answers {named}"
        );
    }

    assert!(prompt(&state(Power::On, false, vec![])).is_none());
}

#[test]
fn the_chip_says_a_question_is_waiting_for_exactly_the_prompts_it_has_to_show() {
    assert_eq!(waiting(&state(Power::On, false, vec![])), None);

    for one in [
        Prompt::Confirm {
            device: pixel(),
            passkey: 0,
        },
        Prompt::Authorize(pixel()),
        Prompt::DisplayPin {
            device: pixel(),
            pin: "0000".to_owned(),
        },
        Prompt::DisplayPasskey {
            device: pixel(),
            passkey: 0,
            entered: 0,
        },
    ] {
        let named = format!("{one:?}");
        assert!(
            waiting(&asking(one)).is_some(),
            "closing the popover on {named} leaves the bar the only way back to it"
        );
    }

    for one in [Prompt::RequestPin(pixel()), Prompt::RequestPasskey(pixel())] {
        let named = format!("{one:?}");
        assert_eq!(
            waiting(&asking(one)),
            None,
            "{named} raises a dialog of its own, so the chip has nothing to add"
        );
    }

    let mut forgetting = state(Power::On, false, vec![device("Pixel", false)]);
    forgetting.confirm = Some(Confirmation::Forget {
        device: pixel(),
        connected: false,
    });
    assert!(waiting(&forgetting).is_some());
}

#[test]
fn a_question_offers_an_answer_and_a_display_offers_only_a_way_out() {
    let confirm = prompt(&asking(Prompt::Confirm {
        device: pixel(),
        passkey: 418_209,
    }))
    .unwrap();
    assert_eq!(confirm.device, "Pixel");
    assert_eq!(confirm.code, "418 209");
    assert!(!confirm.accept.is_empty(), "a question can be answered yes");

    let authorize = prompt(&asking(Prompt::Authorize(pixel()))).unwrap();
    assert!(authorize.code.is_empty());
    assert!(!authorize.accept.is_empty());

    let pin = prompt(&asking(Prompt::DisplayPin {
        device: pixel(),
        pin: "0000".to_owned(),
    }))
    .unwrap();
    assert_eq!(pin.code, "0000");
    assert!(
        pin.accept.is_empty(),
        "nothing on this computer can confirm what was typed on the device"
    );
    assert!(!pin.cancel.is_empty());
}

#[test]
fn a_passkey_is_six_zero_padded_digits_and_its_progress_stops_at_six() {
    let shown = |passkey, entered| {
        prompt(&asking(Prompt::DisplayPasskey {
            device: pixel(),
            passkey,
            entered,
        }))
        .unwrap()
    };

    assert_eq!(shown(18_402, 3).code, "018 402");
    assert_eq!(shown(u32::MAX, 0).code, "999 999");
    assert_eq!(shown(418_209, 3).progress, "3 of 6 entered");
    assert_eq!(
        shown(418_209, 9).progress,
        "6 of 6 entered",
        "a remote count past the passkey length is clamped, never rendered"
    );
}

#[test]
fn an_unnamed_device_is_still_named_on_the_page() {
    let mut state = asking(Prompt::Authorize(DeviceId::new(
        "/org/bluez/hci0/dev_ghost",
    )));
    state.devices.clear();

    assert_eq!(prompt(&state).unwrap().device, "Unknown device");
}

#[test]
fn a_hostile_device_name_is_capped_before_it_reaches_the_page() {
    let long = "Наушники ".repeat(20);
    let mut state = asking(Prompt::Authorize(pixel()));
    state.devices[0].name = long.clone();

    let shown = prompt(&state).unwrap();

    assert_eq!(
        shown.device.chars().count(),
        NAME_CAP + 1,
        "a remote name reaches the page capped, plus the one character marking the cut"
    );
    assert!(shown.device.ends_with('…'));
}
