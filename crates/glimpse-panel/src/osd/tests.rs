use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use glimpse_services::{
    AudioDevice, AudioDeviceId, AudioState, BrightnessKind, BrightnessSource, BrightnessState,
    KeyboardLayout, KeyboardLayouts,
};

use super::{Show, audio_shows, brightness_shows, keyboard_shows, quieted};

fn device(id: &str, port: Option<&str>, volume: u32, muted: bool) -> AudioDevice {
    AudioDevice {
        id: AudioDeviceId::new(id),
        index: 0,
        name: format!("Device {id}"),
        port: port.map(str::to_owned),
        icon_name: None,
        form_factor: None,
        volume,
        muted,
        default: true,
    }
}

fn audio(output: AudioDevice) -> AudioState {
    AudioState {
        outputs: vec![output],
        ..AudioState::default()
    }
}

fn brightness(kind: BrightnessKind, current: u32, max: u32) -> BrightnessState {
    BrightnessState {
        sources: vec![BrightnessSource {
            id: "panel".to_owned(),
            kind,
            connector: None,
            current,
            max,
            floor: 0,
        }],
    }
}

fn layouts(current: u8, count: usize, restored: bool) -> KeyboardLayouts {
    KeyboardLayouts {
        layouts: (0..count)
            .map(|index| KeyboardLayout {
                code: format!("l{index}"),
                name: format!("Layout {index}"),
            })
            .collect(),
        current: Some(current),
        restored,
    }
}

#[test]
fn unchanged_audio_shows_nothing() {
    let state = audio(device("a", None, 40, false));
    assert!(audio_shows(&state, &state).is_empty());
}

#[test]
fn missing_default_shows_nothing() {
    let current = audio(device("a", None, 40, false));
    assert!(audio_shows(&AudioState::default(), &current).is_empty());
}

#[test]
fn volume_change_gives_a_meter() {
    let shows = audio_shows(
        &audio(device("a", None, 40, false)),
        &audio(device("a", None, 45, false)),
    );
    assert_eq!(shows.len(), 1);
    assert_eq!(shows[0].value, Some(0.45));
    assert!(shows[0].label.is_empty());
    assert_eq!(shows[0].icon, "audio-volume-medium-symbolic");
}

#[test]
fn mute_gives_an_empty_meter_with_the_muted_icon() {
    let shows = audio_shows(
        &audio(device("a", None, 45, false)),
        &audio(device("a", None, 45, true)),
    );
    assert_eq!(shows.len(), 1);
    assert_eq!(shows[0].value, Some(0.0));
    assert_eq!(shows[0].icon, "audio-volume-muted-symbolic");
}

#[test]
fn default_device_switch_gives_one_device_row() {
    let shows = audio_shows(
        &audio(device("a", None, 40, false)),
        &audio(device("b", None, 90, false)),
    );
    assert_eq!(shows.len(), 1);
    assert_eq!(shows[0].value, None);
    assert_eq!(shows[0].label, "Device b");
}

#[test]
fn port_change_gives_the_port_label() {
    let shows = audio_shows(
        &audio(device("a", Some("Speakers"), 40, false)),
        &audio(device("a", Some("Headphones"), 40, false)),
    );
    assert_eq!(shows.len(), 1);
    assert_eq!(shows[0].label, "Headphones");
    assert_eq!(shows[0].value, None);
}

#[test]
fn brightness_change_gives_a_fraction() {
    let shows = brightness_shows(
        &brightness(BrightnessKind::Display, 50, 100),
        &brightness(BrightnessKind::Display, 60, 100),
    );
    assert_eq!(shows.len(), 1);
    assert_eq!(shows[0].value, Some(0.6));
    assert_eq!(shows[0].icon, "display-brightness-symbolic");
}

#[test]
fn keyboard_backlight_has_its_own_icon() {
    let shows = brightness_shows(
        &brightness(BrightnessKind::Keyboard, 1, 3),
        &brightness(BrightnessKind::Keyboard, 2, 3),
    );
    assert_eq!(shows[0].icon, "keyboard-brightness-symbolic");
}

#[test]
fn unchanged_or_zero_max_brightness_shows_nothing() {
    let state = brightness(BrightnessKind::Display, 50, 100);
    assert!(brightness_shows(&state, &state).is_empty());
    assert!(
        brightness_shows(
            &brightness(BrightnessKind::Display, 0, 0),
            &brightness(BrightnessKind::Display, 1, 0),
        )
        .is_empty()
    );
}

#[test]
fn layout_switch_gives_the_name() {
    let shows = keyboard_shows(&layouts(0, 2, false), &layouts(1, 2, false));
    assert_eq!(shows.len(), 1);
    assert_eq!(shows[0].label, "Layout 1");
    assert_eq!(shows[0].value, None);
}

#[test]
fn restored_layout_shows_nothing() {
    assert!(keyboard_shows(&layouts(0, 2, false), &layouts(1, 2, true)).is_empty());
}

#[test]
fn single_layout_shows_nothing() {
    assert!(keyboard_shows(&layouts(0, 1, false), &layouts(1, 1, false)).is_empty());
}

#[test]
fn quiet_drops_meters_and_keeps_rows_without_a_value() {
    let shows = vec![
        Show {
            icon: "a".to_owned(),
            label: String::new(),
            value: Some(0.5),
        },
        Show {
            icon: "b".to_owned(),
            label: "Headphones".to_owned(),
            value: None,
        },
    ];
    let quiet = Arc::new(AtomicUsize::new(0));
    assert_eq!(quieted(shows.clone(), &quiet).len(), 2);
    quiet.store(1, std::sync::atomic::Ordering::Relaxed);
    let kept = quieted(shows, &quiet);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].label, "Headphones");
}

#[test]
fn startup_layouts_show_nothing() {
    assert!(keyboard_shows(&KeyboardLayouts::default(), &layouts(0, 2, false)).is_empty());
}

#[test]
fn volume_above_the_limit_is_capped() {
    let shows = audio_shows(
        &audio(device("a", None, 40, false)),
        &audio(device("a", None, 250, false)),
    );
    assert_eq!(shows[0].value, Some(2.0));
}

#[test]
fn output_and_input_changing_together_show_only_the_output() {
    let mut previous = audio(device("a", None, 40, false));
    previous.inputs = vec![device("m", None, 40, false)];
    let mut current = audio(device("b", None, 40, false));
    current.inputs = vec![device("n", None, 40, false)];
    let shows = audio_shows(&previous, &current);
    assert_eq!(shows.len(), 1);
    assert_eq!(shows[0].label, "Device b");
}

#[test]
fn brightness_from_an_empty_state_shows_nothing() {
    assert!(
        brightness_shows(
            &BrightnessState::default(),
            &brightness(BrightnessKind::Display, 50, 100),
        )
        .is_empty()
    );
}

#[test]
fn input_volume_gives_a_meter_with_the_microphone_icon() {
    let input = |volume, muted| AudioState {
        inputs: vec![device("m", None, volume, muted)],
        ..AudioState::default()
    };
    let shows = audio_shows(&input(40, false), &input(55, false));
    assert_eq!(shows.len(), 1);
    assert_eq!(shows[0].value, Some(0.55));
    assert_eq!(shows[0].icon, "microphone-sensitivity-medium-symbolic");

    let shows = audio_shows(&input(55, false), &input(55, true));
    assert_eq!(shows.len(), 1);
    assert_eq!(shows[0].value, Some(0.0));
    assert_eq!(shows[0].icon, "microphone-sensitivity-muted-symbolic");
}
