use gettextrs::{gettext, ngettext};
use glimpse_dbus::bluez::{Codec, DeviceIcon, Power, Profile};
use glimpse_services::{BluetoothState, Busy, Device, DeviceId, Failure};
use glimpse_widgets::{
    BluetoothDetails as Details, BluetoothEntry as Entry, BluetoothLine as Line,
    BluetoothPlace as Place,
};

pub const ACTIVE: &str = "bluetooth-active-symbolic";
pub const IDLE: &str = "bluetooth-symbolic";
pub const SCANNING: &str = "bluetooth-acquiring-symbolic";
pub const OFF: &str = "bluetooth-disabled-symbolic";
pub const BLOCKED: &str = "bluetooth-hardware-disabled-symbolic";

pub const NAME_CAP: usize = 24;
const PROFILES_SHOWN: usize = 3;

pub fn chip(state: &BluetoothState) -> Option<&'static str> {
    let adapter = state.adapter.as_ref()?;
    Some(icon_for(
        adapter.power,
        adapter.discovering,
        state.connected().next().is_some(),
    ))
}

pub fn visible_as(state: &BluetoothState) -> Option<String> {
    let adapter = state
        .adapter
        .as_ref()
        .filter(|adapter| adapter.discoverable)?;
    Some(gettext("Visible to other devices as {name}").replace("{name}", &cap(&adapter.alias)))
}

pub fn tooltip(state: &BluetoothState, format: Option<&str>) -> Option<String> {
    let adapter = state.adapter.as_ref()?;
    let status = status(
        adapter.power,
        adapter.discovering,
        state.connected().count(),
    );
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

pub fn scan_label(scanning: bool) -> String {
    match scanning {
        true => gettext("Stop searching"),
        false => gettext("Add a device"),
    }
}

fn services(profiles: &[Profile]) -> String {
    let shown: Vec<String> = profiles
        .iter()
        .take(PROFILES_SHOWN)
        .map(|profile| profile_name(*profile))
        .collect();
    match profiles.len() > PROFILES_SHOWN {
        true => format!("{}…", shown.join(", ")),
        false => shown.join(", "),
    }
}

pub fn cap(name: &str) -> String {
    glimpse_utils::clean(name, NAME_CAP)
}

pub fn hero(state: &BluetoothState) -> Hero {
    let adapter = state.adapter.as_ref();
    let power = adapter.map_or(Power::Off, |adapter| adapter.power);
    let connected = state.connected().count();
    let discovering = adapter.is_some_and(|adapter| adapter.discovering);

    Hero {
        title: gettext("Bluetooth"),
        subtitle: status(power, discovering, connected),
        icon: icon_for(power, discovering, connected > 0).to_owned(),
        on: matches!(power, Power::On | Power::Enabling),
        settable: power != Power::Blocked,
    }
}

fn icon_for(power: Power, discovering: bool, any_connected: bool) -> &'static str {
    match power {
        Power::Blocked => BLOCKED,
        Power::Off | Power::Disabling => OFF,
        Power::Enabling => IDLE,
        Power::On if discovering => SCANNING,
        Power::On if any_connected => ACTIVE,
        Power::On => IDLE,
    }
}

pub struct Hero {
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub on: bool,
    pub settable: bool,
}

pub struct Listing {
    pub entries: Vec<Entry>,
    pub more_paired: Option<String>,
    pub more_nearby: Option<String>,
}

pub fn entries(
    state: &BluetoothState,
    selected: Option<&DeviceId>,
    devices: usize,
    nearby: usize,
) -> Listing {
    let mut built = Vec::new();
    let chosen = |device: &Device| selected.is_some_and(|id| id == &device.id);

    for device in state.devices.iter().filter(|device| device.connected) {
        built.push(entry(device, Place::Connected, chosen(device)));
    }

    let paired: Vec<&Device> = state
        .devices
        .iter()
        .filter(|device| !device.connected && device.known())
        .collect();
    for device in paired.iter().take(devices) {
        built.push(entry(device, Place::Paired, chosen(device)));
    }

    let found: Vec<&Device> = state
        .devices
        .iter()
        .filter(|device| !device.connected && !device.known())
        .collect();
    for device in found.iter().take(nearby) {
        built.push(entry(device, Place::Nearby, chosen(device)));
    }

    Listing {
        entries: built,
        more_paired: more(paired.len().saturating_sub(devices)),
        more_nearby: more(found.len().saturating_sub(nearby)),
    }
}

fn more(hidden: usize) -> Option<String> {
    (hidden > 0).then(|| {
        ngettext("{count} more device", "{count} more devices", hidden as u32)
            .replace("{count}", &hidden.to_string())
    })
}

fn entry(device: &Device, place: Place, selected: bool) -> Entry {
    Entry {
        id: device.id.as_str().to_owned(),
        title: cap(&device.name),
        subtitle: kind(device.icon),
        icon: icon(device.icon).to_owned(),
        place,
        value: state_of(device),
        selected,
    }
}

fn state_of(device: &Device) -> String {
    match device.busy {
        Some(Busy::Connecting) => return gettext("Connecting…"),
        Some(Busy::Disconnecting) => return gettext("Disconnecting…"),
        Some(Busy::Pairing) => return gettext("Pairing…"),
        Some(Busy::Forgetting) => return gettext("Removing…"),
        None => {}
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
    String::new()
}

pub fn details(state: &BluetoothState, id: &DeviceId) -> Option<Details> {
    let device = state.device(id)?;
    let mut lines = Vec::new();

    if device.known() {
        lines.push(acts(if device.connected {
            ("disconnect", gettext("Disconnect"))
        } else {
            ("connect", gettext("Connect"))
        }));
    } else {
        lines.push(acts(("pair", gettext("Pair this device"))));
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
    if device.known() {
        lines.push(Line {
            action: "trust".to_owned(),
            title: gettext("Connect automatically"),
            toggle: Some(device.trusted),
            ..Default::default()
        });
    }
    lines.push(line(
        ("address", gettext("Address")),
        device.address.clone(),
    ));
    if device.paired != device.bonded {
        lines.push(line(
            ("pairing", gettext("Pairing")),
            gettext("Will not survive a restart"),
        ));
    }
    if let Some(rssi) = device.rssi {
        lines.push(line(("signal", gettext("Signal")), format!("{rssi} dBm")));
    }
    if !device.profiles.is_empty() {
        lines.push(line(
            ("services", gettext("Services")),
            services(&device.profiles),
        ));
    }
    if device.known() {
        lines.push(Line {
            action: "forget".to_owned(),
            title: gettext("Forget this device"),
            icon: "user-trash-symbolic".to_owned(),
            destructive: true,
            activates: true,
            ..Default::default()
        });
    }

    Some(Details {
        id: device.id.as_str().to_owned(),
        notice: device.failure.map(wording).unwrap_or_default(),
        lines,
    })
}

fn line((action, title): (&str, String), value: String) -> Line {
    Line {
        action: action.to_owned(),
        title,
        value,
        icon: String::new(),
        toggle: None,
        destructive: false,
        activates: false,
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
        Failure::PairingRejected => gettext("The device rejected the pairing."),
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

fn profile_name(profile: Profile) -> String {
    match profile {
        Profile::Audio => gettext("Audio"),
        Profile::Calls => gettext("Calls"),
        Profile::RemoteControl => gettext("Remote control"),
        Profile::Input => gettext("Input"),
        Profile::Network => gettext("Network"),
        Profile::FileTransfer => gettext("File transfer"),
        Profile::PhoneBook => gettext("Contacts"),
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

    fn state(power: Power, discovering: bool, devices: Vec<Device>) -> BluetoothState {
        BluetoothState {
            adapter: Some(Adapter {
                alias: "glimpse".to_owned(),
                power,
                discovering,
                discoverable: false,
            }),
            devices,
            scanning: discovering,
            pairing: None,
            confirm: None,
        }
    }

    #[test]
    fn a_machine_with_no_radio_shows_no_chip() {
        assert!(chip(&BluetoothState::default()).is_none());
        assert!(tooltip(&BluetoothState::default(), None).is_none());
    }

    #[test]
    fn the_icon_follows_the_power_state() {
        assert_eq!(chip(&state(Power::Off, false, vec![])).unwrap(), OFF);
        assert_eq!(
            chip(&state(Power::Blocked, false, vec![])).unwrap(),
            BLOCKED
        );
        assert_eq!(
            chip(&state(Power::Enabling, false, vec![])).unwrap(),
            IDLE,
            "a transition shows the state being entered"
        );
        assert_eq!(chip(&state(Power::Disabling, false, vec![])).unwrap(), OFF);
        assert_eq!(chip(&state(Power::On, true, vec![])).unwrap(), SCANNING);
        assert_eq!(chip(&state(Power::On, false, vec![])).unwrap(), IDLE);
    }

    #[test]
    fn the_bar_carries_an_icon_and_never_a_device_name() {
        let one = state(Power::On, false, vec![device("Buds", true)]);
        assert_eq!(chip(&one).unwrap(), ACTIVE);
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
        assert_eq!(chip(&many).unwrap(), ACTIVE);
        assert_eq!(tooltip(&many, None).as_deref(), Some("2 devices connected"));
        assert_eq!(
            tooltip(&state(Power::On, false, vec![]), None).as_deref(),
            Some("No devices connected")
        );
    }

    #[test]
    fn the_caption_names_the_adapter_only_while_it_is_visible() {
        let mut shown = state(Power::On, false, vec![]);
        assert_eq!(
            visible_as(&shown),
            None,
            "an adapter nobody can see says nothing about being seen"
        );

        if let Some(adapter) = shown.adapter.as_mut() {
            adapter.discoverable = true;
        }
        assert_eq!(
            visible_as(&shown).as_deref(),
            Some("Visible to other devices as glimpse")
        );
    }

    #[test]
    fn a_disconnected_device_does_not_light_the_icon() {
        let some = state(
            Power::On,
            false,
            vec![device("Buds", true), device("Mouse", false)],
        );
        assert_eq!(chip(&some).unwrap(), ACTIVE);

        let none = state(Power::On, false, vec![device("Mouse", false)]);
        assert_eq!(chip(&none).unwrap(), IDLE);
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
    fn a_long_profile_list_is_cut_rather_than_eating_the_row_title() {
        assert_eq!(
            services(&[Profile::Audio, Profile::Calls, Profile::RemoteControl]),
            "Audio, Calls, Remote control"
        );
        assert_eq!(
            services(&[
                Profile::Audio,
                Profile::Calls,
                Profile::RemoteControl,
                Profile::Network,
                Profile::FileTransfer,
            ]),
            "Audio, Calls, Remote control…",
            "a row's value has no ellipsize, so an unbounded one squeezes the title to nothing"
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

        let listing = entries(&state, None, 6, 8);
        let counted = |place: Place| {
            listing
                .entries
                .iter()
                .filter(|entry| entry.place == place)
                .count()
        };

        assert_eq!(counted(Place::Connected), 1);
        assert_eq!(
            counted(Place::Paired),
            6,
            "the rest go behind an overflow row, not onto a popover nothing scrolls"
        );
        assert_eq!(counted(Place::Nearby), 8);
        assert!(listing.more_paired.is_some());
        assert!(listing.more_nearby.is_some());
        assert!(
            entries(&state, None, usize::MAX, usize::MAX)
                .more_paired
                .is_none(),
            "an expanded list has nothing left to reach"
        );
    }

    #[test]
    fn a_busy_device_says_so_instead_of_its_battery() {
        let mut busy = device("Buds", false);
        busy.busy = Some(Busy::Connecting);
        busy.battery = Some(80);
        let state = state(Power::On, false, vec![busy]);

        let listing = entries(&state, None, 6, 8);

        assert_eq!(listing.entries[0].value, gettext("Connecting…"));
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
    }

    #[test]
    fn an_unpaired_device_is_offered_pairing_and_nothing_to_forget() {
        let mut found = device("Bose", false);
        found.paired = false;
        found.bonded = false;
        found.trusted = false;
        found.rssi = Some(-73);
        let id = found.id.clone();
        let state = state(Power::On, true, vec![found]);

        let details = details(&state, &id).expect("the device");
        let actions: Vec<&str> = details
            .lines
            .iter()
            .map(|line| line.action.as_str())
            .collect();

        assert!(actions.contains(&"pair"));
        assert!(!actions.contains(&"forget"));
        assert!(!actions.contains(&"trust"));
        assert!(actions.contains(&"signal"));
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
    fn a_pairing_row_appears_only_when_paired_and_bonded_disagree() {
        let mut fragile = device("Buds", false);
        fragile.bonded = false;
        let id = fragile.id.clone();
        let state = state(Power::On, false, vec![fragile]);

        assert!(
            details(&state, &id)
                .expect("the device")
                .lines
                .iter()
                .any(|line| line.action == "pairing")
        );
    }

    #[test]
    fn a_failure_reaches_the_page_as_a_sentence_and_never_as_a_token() {
        let mut broken = device("Buds", false);
        broken.failure = Some(Failure::Unreachable);
        let id = broken.id.clone();
        let state = state(Power::On, false, vec![broken]);

        let notice = details(&state, &id).expect("the device").notice;

        assert!(!notice.is_empty());
        assert!(!notice.contains("br-connection"));
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
}
