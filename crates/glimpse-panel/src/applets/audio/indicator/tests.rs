use super::*;

fn device(id: &str, volume: u32, muted: bool, default: bool) -> AudioDevice {
    AudioDevice {
        id: AudioDeviceId::new(id),
        index: 0,
        name: id.to_owned(),
        icon_name: None,
        form_factor: None,
        volume,
        muted,
        default,
    }
}

fn role(device_id: &str, volume: u32, muted: bool, adjustable: bool) -> AudioRole {
    AudioRole {
        volume,
        muted,
        adjustable,
        device: AudioDeviceId::new(device_id),
        corked: false,
        streams: Vec::new(),
    }
}

fn app(id: &str, playback: Option<AudioRole>, capture: Option<AudioRole>) -> AudioApp {
    AudioApp {
        id: AudioAppId::new(id),
        name: id.to_owned(),
        icon_name: None,
        app_id: None,
        binary: None,
        playback,
        capture,
    }
}

#[test]
fn direction_of_only_accepts_the_two_spellings_the_widget_emits() {
    assert_eq!(direction_of("output"), Some(AudioDirection::Output));
    assert_eq!(direction_of("input"), Some(AudioDirection::Input));
    assert_eq!(direction_of("garbage"), None);
}

#[test]
fn device_entry_is_selected_only_for_the_default_device() {
    assert!(device_entry(&device("dev", 50, false, true), AudioDirection::Output).selected);
    assert!(!device_entry(&device("dev", 50, false, false), AudioDirection::Output).selected);
}

#[test]
fn device_entries_lists_every_device_with_the_default_first() {
    let devices = vec![
        device("onboard", 40, false, false),
        device("hdmi", 40, false, false),
        device("dock", 40, false, false),
        device("headset", 40, false, false),
        device("speakers", 40, false, true),
    ];
    let shown = device_entries(&devices, AudioDirection::Output);
    assert_eq!(
        shown.len(),
        5,
        "the devices live in a card opened only to switch, so nothing caps them"
    );
    assert!(shown[0].selected, "the default sorts to the front");
}

#[test]
fn a_block_icon_reflects_the_apps_own_volume_not_its_devices() {
    let quiet = role("dev", 0, false, true);
    let loud = role("dev", 100, false, true);
    let quiet_block = block(
        &AudioState::default(),
        AudioDirection::Output,
        &quiet,
        false,
    );
    let loud_block = block(&AudioState::default(), AudioDirection::Output, &loud, false);
    assert_ne!(quiet_block.icon, loud_block.icon);
}

#[test]
fn a_muted_app_carries_the_muted_glyph_of_its_own_direction() {
    let playing = app("firefox", Some(role("dev", 80, false, true)), None);
    assert_eq!(app_entry(&playing).muted_icon, None);

    let silenced = app("firefox", Some(role("dev", 80, true, true)), None);
    assert_eq!(
        app_entry(&silenced).muted_icon.as_deref(),
        Some(render::fader_icon(AudioDirection::Output, 80, true))
    );

    let recording = app("obs", None, Some(role("mic", 40, true, true)));
    assert_eq!(
        app_entry(&recording).muted_icon.as_deref(),
        Some(render::fader_icon(AudioDirection::Input, 40, true)),
        "an app that only records is muted as a microphone, not a speaker"
    );
}

#[test]
fn details_is_none_for_an_app_that_has_vanished() {
    let state = AudioState::default();
    assert!(details(&state, &AudioAppId::new("ghost")).is_none());
}

#[test]
fn a_single_role_application_carries_no_heading() {
    let state = AudioState {
        apps: vec![app("firefox", Some(role("dev", 80, false, true)), None)],
        ..AudioState::default()
    };
    let found = details(&state, &AudioAppId::new("firefox")).expect("the app exists");
    assert_eq!(found.blocks.len(), 1);
    assert!(found.blocks[0].heading.is_none());
}

#[test]
fn a_two_role_application_carries_output_then_input_headings() {
    let state = AudioState {
        apps: vec![app(
            "obs",
            Some(role("dev", 40, false, true)),
            Some(role("mic", 20, true, false)),
        )],
        ..AudioState::default()
    };
    let found = details(&state, &AudioAppId::new("obs")).expect("the app exists");
    assert_eq!(found.blocks.len(), 2);
    assert_eq!(found.blocks[0].heading.as_deref(), Some("Output"));
    assert_eq!(found.blocks[1].heading.as_deref(), Some("Input"));
}

#[test]
fn a_block_marks_the_device_the_app_is_currently_routed_to() {
    let state = AudioState {
        outputs: vec![
            device("dev-a", 100, false, true),
            device("dev-b", 100, false, false),
        ],
        apps: vec![app("firefox", Some(role("dev-b", 80, false, true)), None)],
        ..AudioState::default()
    };
    let found = details(&state, &AudioAppId::new("firefox")).expect("the app exists");
    let devices = &found.blocks[0].devices;
    assert!(
        devices
            .iter()
            .find(|entry| entry.id == "dev-b")
            .expect("dev-b is listed")
            .selected
    );
    assert!(
        !devices
            .iter()
            .find(|entry| entry.id == "dev-a")
            .expect("dev-a is listed")
            .selected
    );
}
