use gettextrs::{gettext, ngettext};
use glimpse_dbus::bluez::{Codec, DeviceIcon, Power};
use glimpse_services::{BluetoothState, Busy, Confirmation, Device, DeviceId, Failure, Prompt};
use glimpse_widgets::{
    BluetoothAsk as Ask, BluetoothDetails as Details, BluetoothEntry as Entry,
    BluetoothLine as Line, BluetoothPlace as Place, PASSKEY_MAX, PIN_MAX, PairingEntry,
};

pub const IDLE: &str = "bluetooth-symbolic";

pub const NAME_CAP: usize = 24;

pub fn tooltip(state: &BluetoothState, format: Option<&str>) -> Option<String> {
    let adapter = state.adapter.as_ref()?;
    let status = status(adapter.power, state.held(), state.connected().count());
    let Some(format) = format else {
        return Some(status);
    };

    let devices = state
        .connected()
        .map(|device| cap(&device.name))
        .collect::<Vec<_>>()
        .join(", ");
    let alias = cap(&adapter.alias);
    Some(crate::applets::tokens::render(
        format,
        |token| match token {
            "status" => Some(status.as_str()),
            "devices" => Some(devices.as_str()),
            "adapter" => Some(alias.as_str()),
            _ => None,
        },
    ))
}

fn status(power: Power, discovering: bool, connected: usize) -> String {
    match power {
        Power::Blocked => gettext("Bluetooth is blocked"),
        Power::Off => gettext("Bluetooth is off"),
        Power::Enabling => gettext("Turning Bluetooth on"),
        Power::Disabling => gettext("Turning Bluetooth off"),
        Power::On if discovering => gettext("Looking for devices"),
        Power::On => match connected {
            0 => gettext("No devices connected"),
            count => ngettext(
                "{count} device connected",
                "{count} devices connected",
                count as u32,
            )
            .replace("{count}", &count.to_string()),
        },
    }
}

pub fn cap(name: &str) -> String {
    glimpse_utils::clean(name, NAME_CAP)
}

pub fn hero(state: &BluetoothState) -> Hero {
    let adapter = state.adapter.as_ref();
    let power = adapter.map_or(Power::Off, |adapter| adapter.power);
    let connected = state.connected().count();
    let searching = state.scanning();

    Hero {
        title: gettext("Bluetooth"),
        subtitle: status(power, searching, connected),
        icon: glimpse_services::icon_for(power, searching, connected > 0).to_owned(),
        on: matches!(power, Power::On | Power::Enabling),
        settable: power != Power::Blocked,
        controls: powered(state),
    }
}

pub fn typed(prompt: &Prompt) -> Option<PairingEntry> {
    match prompt {
        Prompt::RequestPin(_) => Some(PairingEntry::Pin),
        Prompt::RequestPasskey(_) => Some(PairingEntry::Passkey),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    Pairing(DeviceId),
    Forget(DeviceId),
}

impl Asked {
    fn key(&self) -> String {
        match self {
            Self::Pairing(device) => format!("pair:{}", device.as_str()),
            Self::Forget(device) => format!("forget:{}", device.as_str()),
        }
    }
}

pub fn asked(state: &BluetoothState) -> Option<Asked> {
    if let Some(prompt) = state
        .pairing
        .as_ref()
        .filter(|prompt| typed(prompt).is_none())
    {
        return Some(Asked::Pairing(prompt.device().clone()));
    }
    let Confirmation::Forget { device, .. } = state.confirm.as_ref()?;
    Some(Asked::Forget(device.clone()))
}

pub fn powered(state: &BluetoothState) -> bool {
    state
        .adapter
        .as_ref()
        .is_some_and(|adapter| adapter.power == Power::On)
}

pub fn waiting(state: &BluetoothState) -> Option<String> {
    asked(state).map(|_| gettext("Bluetooth is waiting for an answer"))
}

pub fn prompt(state: &BluetoothState) -> Option<Ask> {
    pairing(state).or_else(|| confirmation(state))
}

fn confirmation(state: &BluetoothState) -> Option<Ask> {
    let Confirmation::Forget { device, connected } = state.confirm.as_ref()?;
    Some(Ask {
        key: Asked::Forget(device.clone()).key(),
        device: named(state, device),
        question: match connected {
            true => gettext(
                "It will be disconnected, and this computer will stop connecting to it. To use it again you will have to pair it, with the device in pairing mode.",
            ),
            false => gettext(
                "This computer will stop connecting to it, and it will not connect on its own. To use it again you will have to pair it, with the device in pairing mode.",
            ),
        },
        code: String::new(),
        progress: String::new(),
        accept: gettext("Forget"),
        destructive: true,
        cancel: gettext("Cancel"),
    })
}

fn named(state: &BluetoothState, device: &DeviceId) -> String {
    state
        .name(device)
        .filter(|name| !name.is_empty())
        .map_or_else(|| gettext("Unknown device"), cap)
}

fn pairing(state: &BluetoothState) -> Option<Ask> {
    let prompt = state.pairing.as_ref()?;
    let key = Asked::Pairing(prompt.device().clone()).key();
    let device = named(state, prompt.device());

    Some(match prompt {
        Prompt::Confirm { passkey, .. } => Ask {
            key: key.clone(),
            device,
            question: gettext("Is this the code shown on the device?"),
            code: digits(*passkey),
            progress: String::new(),
            accept: gettext("Confirm"),
            destructive: false,
            cancel: gettext("Cancel"),
        },
        Prompt::Authorize(_) => Ask {
            key: key.clone(),
            device,
            question: gettext("This device wants to pair with this computer."),
            code: String::new(),
            progress: String::new(),
            accept: gettext("Allow"),
            destructive: false,
            cancel: gettext("Deny"),
        },
        Prompt::DisplayPin { pin, .. } => Ask {
            key: key.clone(),
            device,
            question: gettext("Type this PIN on the device. It will not ask again."),
            code: glimpse_utils::clean(pin, PIN_MAX),
            progress: String::new(),
            accept: String::new(),
            destructive: false,
            cancel: gettext("Cancel"),
        },
        Prompt::DisplayPasskey {
            passkey, entered, ..
        } => Ask {
            key: key.clone(),
            device,
            question: gettext("Type this passkey on the device."),
            code: digits(*passkey),
            progress: gettext("{entered} of 6 entered")
                .replace("{entered}", &(*entered).min(6).to_string()),
            accept: String::new(),
            destructive: false,
            cancel: gettext("Cancel"),
        },
        Prompt::RequestPin(_) | Prompt::RequestPasskey(_) => return None,
    })
}

fn digits(passkey: u32) -> String {
    let padded = format!("{:06}", passkey.min(PASSKEY_MAX));
    format!("{} {}", &padded[..3], &padded[3..])
}

pub struct Hero {
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub controls: bool,
    pub on: bool,
    pub settable: bool,
}

pub struct Listing {
    pub entries: Vec<Entry>,
    pub more_paired: Option<String>,
    pub more_nearby: Option<String>,
    pub nearby_count: usize,
    pub nearby_open: bool,
}

/// Devices, then Nearby devices as the same disclosure the network popover gives Other networks:
/// closed while anything is paired, because that is what a person reaches for, and open by itself
/// when nothing is. `toggled` is the header pressed against that default. The popover's own timed
/// scan fills it; the list never waits on one, so it is never shown empty.
pub fn entries(
    state: &BluetoothState,
    devices: usize,
    nearby: usize,
    expanded: (bool, bool),
    toggled: bool,
) -> Listing {
    let (all_paired, all_nearby) = expanded;
    let mut built = Vec::new();

    for device in state.devices.iter().filter(|device| device.connected) {
        built.push(entry(device, Place::Connected));
    }

    let paired: Vec<&Device> = state
        .devices
        .iter()
        .filter(|device| !device.connected && device.known())
        .collect();
    for device in paired.iter().take(shown(devices, all_paired)) {
        built.push(entry(device, Place::Paired));
    }

    let found: Vec<&Device> = state
        .devices
        .iter()
        .filter(|device| !device.connected && !device.known())
        .collect();
    let open = built.is_empty() != toggled;
    for device in found.iter().take(shown(nearby, all_nearby)) {
        built.push(entry(device, Place::Nearby));
    }
    let hidden = found.len().saturating_sub(shown(nearby, all_nearby));

    Listing {
        entries: built,
        more_paired: more(paired.len(), devices, all_paired),
        more_nearby: (hidden > 0).then(|| {
            ngettext("{count} more device", "{count} more devices", hidden as u32)
                .replace("{count}", &hidden.to_string())
        }),
        nearby_count: found.len(),
        nearby_open: open,
    }
}

fn shown(cap: usize, expanded: bool) -> usize {
    match expanded {
        true => usize::MAX,
        false => cap,
    }
}

fn more(total: usize, cap: usize, expanded: bool) -> Option<String> {
    let hidden = total.saturating_sub(cap);
    if hidden == 0 {
        return None;
    }
    Some(match expanded {
        true => gettext("Show fewer"),
        false => ngettext("{count} more device", "{count} more devices", hidden as u32)
            .replace("{count}", &hidden.to_string()),
    })
}

fn entry(device: &Device, place: Place) -> Entry {
    let fragile = device.paired != device.bonded;
    Entry {
        id: device.id.as_str().to_owned(),
        title: cap(&device.name),
        subtitle: match fragile {
            true => gettext("Pairing will not survive a restart"),
            false => kind(device.icon),
        },
        warning: fragile,
        icon: icon(device.icon).to_owned(),
        place,
        value: state_of(device),
        busy: device.busy.is_some(),
    }
}

fn state_of(device: &Device) -> String {
    if device.busy.is_some() {
        return String::new();
    }
    if device.connected {
        return device
            .battery
            .map(|level| gettext("{level}%").replace("{level}", &level.to_string()))
            .unwrap_or_default();
    }
    if device.blocked {
        return gettext("Blocked");
    }
    if device.failure == Some(Failure::BondBroken) {
        return gettext("Pairing lost");
    }
    String::new()
}

/// The card under a paired device, or `None` for a nearby one, which has nothing to manage before
/// it pairs. A device in use carries Disconnect; every paired one carries what a person acts on —
/// its battery and codec when BlueZ has them, how it reconnects, and Forget. Address, signal and
/// services are diagnostics, and a pairing that will not survive a restart is on the row itself.
pub fn details(state: &BluetoothState, id: &DeviceId) -> Option<Details> {
    let device = state.device(id)?;
    if !device.known() {
        return None;
    }
    let mut lines = Vec::new();

    if device.connected {
        let mut act = acts(("disconnect", gettext("Disconnect")));
        act.busy = matches!(device.busy, Some(Busy::Disconnecting));
        lines.push(act);
    }
    if let Some(level) = device.battery {
        lines.push(line(
            ("battery", gettext("Battery")),
            gettext("{level}%").replace("{level}", &level.to_string()),
        ));
    }
    if let Some(codec) = device.codec.filter(|_| device.connected) {
        lines.push(line(("codec", gettext("Codec")), codec_name(codec)));
    }
    lines.push(Line {
        action: "trust".to_owned(),
        title: gettext("Connect automatically"),
        toggle: Some(device.trusted),
        ..Default::default()
    });
    lines.push(Line {
        action: "forget".to_owned(),
        title: gettext("Forget this device"),
        activates: true,
        destructive: true,
        busy: matches!(device.busy, Some(Busy::Forgetting)),
        ..Default::default()
    });

    Some(Details {
        id: device.id.as_str().to_owned(),
        lines,
    })
}

fn line((action, title): (&str, String), value: String) -> Line {
    Line {
        action: action.to_owned(),
        title,
        value,
        toggle: None,
        activates: false,
        busy: false,
        destructive: false,
    }
}

fn acts((action, title): (&str, String)) -> Line {
    Line {
        activates: true,
        ..line((action, title), String::new())
    }
}

pub fn wording(failure: Failure) -> String {
    match failure {
        Failure::Unreachable => gettext("The device is switched off or out of range."),
        Failure::Refused => gettext("The device refused the connection."),
        Failure::NoService => gettext("The device offers nothing this computer can connect to."),
        Failure::WrongKey => gettext("The PIN or passkey did not match."),
        Failure::PairingRejected => {
            gettext("The device rejected the pairing. Put it in pairing mode and try again.")
        }
        Failure::PairingTimeout => gettext("The pairing timed out."),
        Failure::PairingCanceled => gettext("The pairing was cancelled."),
        Failure::NoAgent => gettext(
            "This panel could not register a pairing agent, so it cannot pair. Devices that are already paired still connect.",
        ),
        Failure::Busy => gettext("The device is busy with something else."),
        Failure::NotReady => gettext("Bluetooth is switched off."),
        Failure::Dropped => gettext("The device disconnected."),
        Failure::BondBroken => gettext("The pairing with this device is no longer valid."),
        Failure::LocalSetup => gettext(
            "This computer could not open a connection to the device. Try again, or switch Bluetooth off and on.",
        ),
        Failure::Unknown => gettext("Bluetooth could not complete that."),
    }
}

pub fn icon(icon: DeviceIcon) -> &'static str {
    match icon {
        DeviceIcon::Headset => "audio-headset-symbolic",
        DeviceIcon::Headphones => "audio-headphones-symbolic",
        DeviceIcon::Speakers => "audio-speakers-symbolic",
        DeviceIcon::Keyboard => "input-keyboard-symbolic",
        DeviceIcon::Mouse => "input-mouse-symbolic",
        DeviceIcon::Gamepad => "input-gaming-symbolic",
        DeviceIcon::Tablet => "input-tablet-symbolic",
        DeviceIcon::Phone => "phone-symbolic",
        DeviceIcon::Computer => "computer-symbolic",
        DeviceIcon::Display => "video-display-symbolic",
        DeviceIcon::Printer => "printer-symbolic",
        DeviceIcon::Unknown => "bluetooth-symbolic",
    }
}

pub fn kind(icon: DeviceIcon) -> String {
    match icon {
        DeviceIcon::Headset => gettext("Headset"),
        DeviceIcon::Headphones => gettext("Headphones"),
        DeviceIcon::Speakers => gettext("Speaker"),
        DeviceIcon::Keyboard => gettext("Keyboard"),
        DeviceIcon::Mouse => gettext("Mouse"),
        DeviceIcon::Gamepad => gettext("Gamepad"),
        DeviceIcon::Tablet => gettext("Tablet"),
        DeviceIcon::Phone => gettext("Phone"),
        DeviceIcon::Computer => gettext("Computer"),
        DeviceIcon::Display => gettext("Display"),
        DeviceIcon::Printer => gettext("Printer"),
        DeviceIcon::Unknown => gettext("Device"),
    }
}

fn codec_name(codec: Codec) -> String {
    match codec {
        Codec::Sbc => "SBC".to_owned(),
        Codec::Mp3 => "MP3".to_owned(),
        Codec::Aac => "AAC".to_owned(),
        Codec::Atrac => "ATRAC".to_owned(),
        Codec::Ldac => "LDAC".to_owned(),
        Codec::AptX => "aptX".to_owned(),
        Codec::AptXHd => "aptX HD".to_owned(),
        Codec::Lhdc => "LHDC".to_owned(),
        Codec::Vendor => gettext("Vendor codec"),
    }
}

#[cfg(test)]
mod tests {
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
    fn a_device_that_lost_its_bond_says_so_where_a_battery_would_be() {
        let mut dropped = device("Pixel", false);
        dropped.failure = Some(Failure::BondBroken);
        let state = state(Power::On, false, vec![dropped]);

        let row = &entries(&state, 6, 8, (false, false), false).entries[0];

        assert_eq!(
            row.value, "Pairing lost",
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
            assert!(
                row.value.is_empty(),
                "{doing:?} left a word beside the spinner that says the same thing"
            );
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
        let state = state(Power::On, false, vec![connected]);

        let details = details(&state, &id).expect("the device");
        let actions: Vec<&str> = details
            .lines
            .iter()
            .map(|line| line.action.as_str())
            .collect();

        assert!(actions.contains(&"disconnect"));
        assert!(!actions.contains(&"connect"));
        assert!(actions.contains(&"codec"));
        assert!(actions.contains(&"battery"));
        assert!(actions.contains(&"forget"));
        let red: Vec<&str> = details
            .lines
            .iter()
            .filter(|line| line.destructive)
            .map(|line| line.action.as_str())
            .collect();
        assert_eq!(red, ["forget"], "Disconnect beside Forget stays plain");
    }

    #[test]
    fn a_nearby_device_has_no_card() {
        let mut found = device("Bose", false);
        found.paired = false;
        found.bonded = false;
        found.trusted = false;
        found.rssi = Some(-73);
        let id = found.id.clone();
        let state = state(Power::On, true, vec![found]);

        assert!(
            details(&state, &id).is_none(),
            "nothing is managed before a device pairs, so its row pairs and carries no card"
        );
    }

    #[test]
    fn a_codec_row_exists_only_while_connected() {
        let mut idle = device("Buds", false);
        idle.codec = Some(Codec::Ldac);
        let id = idle.id.clone();
        let state = state(Power::On, false, vec![idle]);

        let details = details(&state, &id).expect("the device");

        assert!(
            !details.lines.iter().any(|line| line.action == "codec"),
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
            warned[0].warning && !warned[0].subtitle.is_empty(),
            "a problem is seen without opening anything"
        );
        assert!(calm.iter().all(|entry| !entry.warning));
        assert!(
            !details(&state, &id)
                .expect("the device")
                .lines
                .iter()
                .any(|line| line.action == "pairing"),
            "and the card does not repeat it"
        );
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
}
