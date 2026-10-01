use super::*;

#[test]
fn percent_round_trips_at_the_boundaries() {
    for percent in [0, 40, 50, 100] {
        let volume = percent_to_volume(percent);
        assert_eq!(volume_to_percent(volume), percent);
    }
}

#[test]
fn volume_to_percent_rounds_rather_than_floors() {
    assert_eq!(volume_to_percent(Volume(55050)), 84);
    assert_eq!(volume_to_percent(Volume(13107)), 20);
    assert_eq!(volume_to_percent(Volume(26214)), 40);
}

#[test]
fn percent_to_volume_clamps_instead_of_wrapping() {
    assert_eq!(percent_to_volume(u32::MAX), Volume::MAX);
}

#[test]
fn scale_preserves_an_imbalanced_stereo_mix() {
    let mut volume = ChannelVolumes::default();
    volume.set_len(2);
    volume.get_mut()[0] = percent_to_volume(100);
    volume.get_mut()[1] = percent_to_volume(50);

    volume.scale(percent_to_volume(50)).expect("scale succeeds");

    assert_eq!(volume_to_percent(volume.get()[0]), 50);
    assert_eq!(volume_to_percent(volume.get()[1]), 25);
}

fn raw_device(index: u32, pa_name: &str, is_monitor: bool) -> RawDevice {
    RawDevice {
        index,
        pa_name: pa_name.to_owned(),
        label: pa_name.to_owned(),
        icon_name: None,
        form_factor: None,
        volume: 100,
        muted: false,
        is_monitor,
    }
}

#[test]
fn devices_from_raw_flags_the_matching_default() {
    let raw = vec![
        raw_device(62, "alsa_output.pci-0000_66_00.6.analog-stereo", false),
        raw_device(2736, "bluez_output.F8_4E_17_BC_EE_D5.1", false),
    ];

    let devices = devices_from_raw(raw, Some("bluez_output.F8_4E_17_BC_EE_D5.1"));

    assert_eq!(devices.len(), 2);
    assert!(!devices[0].default);
    assert!(devices[1].default);
    assert_eq!(
        devices[1].id,
        DeviceId::new("bluez_output.F8_4E_17_BC_EE_D5.1")
    );
}

#[test]
fn devices_from_raw_drops_a_source_monitoring_a_sink() {
    let raw = vec![
        raw_device(63, "alsa_input.pci-0000_66_00.6.analog-stereo", false),
        raw_device(
            62,
            "alsa_output.pci-0000_66_00.6.analog-stereo.monitor",
            true,
        ),
    ];

    let devices = devices_from_raw(raw, Some("alsa_input.pci-0000_66_00.6.analog-stereo"));

    assert_eq!(devices.len(), 1);
    assert_eq!(
        devices[0].id,
        DeviceId::new("alsa_input.pci-0000_66_00.6.analog-stereo")
    );
    assert!(devices[0].default);
}

#[test]
fn devices_from_raw_does_not_cap_the_pa_name() {
    let long_name = "alsa_output.usb-Focusrite_Scarlett_Solo_USB_Y7DGS9J06E5D2A-00.analog-stereo";
    assert!(long_name.len() > NAME_CAP);
    let raw = vec![raw_device(1, long_name, false)];

    let devices = devices_from_raw(raw, Some(long_name));

    assert_eq!(devices[0].id, DeviceId::new(long_name));
    assert!(
        devices[0].default,
        "a capped pa_name would never match the (also capped) default"
    );
}

#[test]
fn proplist_map_drops_an_oversized_key_rather_than_truncating_it() {
    let mut proplist = Proplist::new().expect("a proplist");
    let long_key = "x".repeat(PROP_KEY_CAP + 1);
    proplist
        .set_str(&long_key, "value")
        .expect("set the long key");
    proplist
        .set_str("short.key", "kept")
        .expect("set the short key");

    let map = proplist_to_map(&proplist);

    assert!(!map.contains_key(&long_key));
    assert_eq!(map.get("short.key").map(String::as_str), Some("kept"));
}

#[tokio::test]
#[ignore = "needs a running pulse or pipewire-pulse server"]
async fn a_live_snapshot_matches_pactl() {
    use std::process::Command;

    use futures_util::StreamExt;

    let sinks_output = Command::new("pactl")
        .args(["list", "short", "sinks"])
        .output()
        .expect("pactl is available");
    let expected_sink_count = String::from_utf8_lossy(&sinks_output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();

    let default_output = Command::new("pactl")
        .args(["get-default-sink"])
        .output()
        .expect("pactl is available");
    let expected_default = String::from_utf8_lossy(&default_output.stdout)
        .trim()
        .to_owned();

    let mut events = Box::pin(connect().await);
    let mut client = None;
    let mut snapshot = None;
    while let Some(event) = events.next().await {
        match event {
            Event::Ready(c) => client = Some(c),
            Event::Snapshot(s) => {
                snapshot = Some(s);
                break;
            }
            Event::Gone(reason) => panic!("the pulse bridge reported Gone: {reason}"),
        }
    }
    let _ = client;
    let snapshot = snapshot.expect("a snapshot before the event stream ended");

    assert_eq!(
        snapshot.outputs.len(),
        expected_sink_count,
        "sink count disagrees with `pactl list short sinks`"
    );
    let default = snapshot
        .outputs
        .iter()
        .find(|device| device.default)
        .expect("a default sink flagged");
    assert_eq!(default.id, DeviceId::new(expected_default));
}
