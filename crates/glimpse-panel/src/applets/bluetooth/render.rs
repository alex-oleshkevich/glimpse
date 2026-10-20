use gettextrs::{gettext, ngettext};
use glimpse_dbus::bluez::{Codec, DeviceIcon, Power};
use glimpse_services::{BluetoothState, Busy, Confirmation, Device, DeviceId, Failure, Prompt};
use glimpse_widgets::{
    BluetoothAsk as Ask, BluetoothDetails as Details, BluetoothEntry as Entry,
    BluetoothLine as Line, BluetoothPlace as Place, Fact, PASSKEY_MAX, PIN_MAX, PairingEntry,
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
        .map(|device| glimpse_utils::clean(&device.name, NAME_CAP))
        .collect::<Vec<_>>()
        .join(", ");
    let alias = glimpse_utils::clean(&adapter.alias, NAME_CAP);
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

pub fn hero(state: &BluetoothState) -> Hero {
    let adapter = state.adapter.as_ref();
    let power = adapter.map_or(Power::Off, |adapter| adapter.power);

    Hero {
        title: gettext("Bluetooth"),
        on: matches!(power, Power::On | Power::Enabling),
        settable: power != Power::Blocked,
        controls: powered(state),
    }
}

pub fn prompt_entry(prompt: &Prompt) -> (PairingEntry, String) {
    match prompt {
        Prompt::RequestPin(_) => (PairingEntry::Pin, String::new()),
        Prompt::RequestPasskey(_) => (PairingEntry::Passkey, String::new()),
        Prompt::Confirm { passkey, .. } => (PairingEntry::Confirm, digits(*passkey)),
        Prompt::Authorize(_) => (PairingEntry::Authorize, String::new()),
        Prompt::DisplayPin { pin, .. } => {
            (PairingEntry::ShowPin, glimpse_utils::clean(pin, PIN_MAX))
        }
        Prompt::DisplayPasskey {
            passkey, entered, ..
        } => (
            PairingEntry::ShowPasskey,
            format!(
                "{}\n{}",
                digits(*passkey),
                gettext("{entered} of 6 entered")
                    .replace("{entered}", &(*entered).min(6).to_string())
            ),
        ),
    }
}

pub fn asked(state: &BluetoothState) -> Option<DeviceId> {
    let Confirmation::Forget { device, .. } = state.confirm.as_ref()?;
    Some(device.clone())
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
    let Confirmation::Forget { device, connected } = state.confirm.as_ref()?;
    Some(Ask {
        key: format!("forget:{}", device.as_str()),
        device: named(state, device),
        question: match connected {
            true => gettext(
                "It will be disconnected, and this computer will stop connecting to it. To use it again you will have to pair it, with the device in pairing mode.",
            ),
            false => gettext(
                "This computer will stop connecting to it, and it will not connect on its own. To use it again you will have to pair it, with the device in pairing mode.",
            ),
        },
        accept: gettext("Forget"),
        destructive: true,
        cancel: gettext("Cancel"),
    })
}

fn named(state: &BluetoothState, device: &DeviceId) -> String {
    state
        .name(device)
        .filter(|name| !name.is_empty())
        .map_or_else(
            || gettext("Unknown device"),
            |name| glimpse_utils::clean(name, NAME_CAP),
        )
}

fn digits(passkey: u32) -> String {
    let padded = format!("{:06}", passkey.min(PASSKEY_MAX));
    format!("{} {}", &padded[..3], &padded[3..])
}

pub struct Hero {
    pub title: String,
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
    Entry {
        id: device.id.as_str().to_owned(),
        title: glimpse_utils::clean(&device.name, NAME_CAP),
        problem: problem(device),
        icon: icon(device.icon).to_owned(),
        place,
        busy: device.busy.is_some(),
    }
}

fn battery(device: &Device) -> Option<String> {
    device
        .battery
        .map(|level| gettext("{level}%").replace("{level}", &level.to_string()))
}

/// What is wrong with a device, said on its row: blocked, a pairing that is lost, or one that will
/// not survive a restart. A device with nothing wrong has no second line.
fn problem(device: &Device) -> String {
    if device.blocked {
        return gettext("Blocked");
    }
    if device.failure == Some(Failure::BondBroken) {
        return gettext("Pairing lost");
    }
    if device.paired != device.bonded {
        return gettext("Pairing will not survive a restart");
    }
    String::new()
}

pub fn details(state: &BluetoothState, id: &DeviceId) -> Option<Details> {
    let device = state.device(id)?;
    let mut facts = vec![Fact::new(gettext("Type"), kind(device.icon))];
    facts.extend(battery(device).map(|level| Fact::new(gettext("Battery"), level)));
    if let Some(codec) = device.codec.filter(|_| device.connected) {
        facts.push(Fact::new(gettext("Codec"), codec_name(codec)));
    }
    let mut lines = vec![Line {
        action: if device.connected {
            "disconnect"
        } else if device.known() {
            "connect"
        } else {
            "pair"
        }
        .to_owned(),
        title: if device.connected {
            gettext("Disconnect")
        } else if device.known() {
            gettext("Connect")
        } else {
            gettext("Pair")
        },
        activates: true,
        busy: matches!(
            device.busy,
            Some(Busy::Connecting | Busy::Disconnecting | Busy::Pairing)
        ),
        ..Default::default()
    }];
    if device.known() {
        lines.push(Line {
            action: "trust".to_owned(),
            title: gettext("Connect automatically"),
            toggle: Some(device.trusted),
            ..Default::default()
        });
    }
    if device.known() {
        lines.push(Line {
            action: "forget".to_owned(),
            title: gettext("Forget this device"),
            activates: true,
            destructive: true,
            busy: matches!(device.busy, Some(Busy::Forgetting)),
            ..Default::default()
        });
    }
    Some(Details {
        id: device.id.as_str().to_owned(),
        facts,
        lines,
    })
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
        Failure::Busy => gettext("The device is busy, possibly connected to another device."),
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
mod tests;
