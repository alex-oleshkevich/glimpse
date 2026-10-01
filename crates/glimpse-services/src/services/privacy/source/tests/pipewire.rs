use super::*;

#[test]
fn a_corked_capture_is_excluded() {
    let state = AudioState {
        inputs: vec![device("mic")],
        apps: vec![app("chrome", "chrome", None, Some(role("mic", true)))],
        ..Default::default()
    };

    assert!(microphone(&state).is_empty());
}

#[test]
fn a_capture_on_a_monitor_source_is_excluded() {
    let state = AudioState {
        inputs: vec![device("mic")],
        apps: vec![app(
            "recorder",
            "recorder",
            None,
            Some(role("bluez_output.monitor", false)),
        )],
        ..Default::default()
    };

    assert!(
        microphone(&state).is_empty(),
        "a monitor device is not present in AudioState.inputs, and that absence is the signal"
    );
}

#[test]
fn a_mixed_monitor_and_mic_app_still_reports_the_mic() {
    let mut role = role("bluez_output.monitor", false);
    role.streams.push(stream_ref("mic"));
    let state = AudioState {
        inputs: vec![device("mic")],
        apps: vec![app("chrome", "chrome", None, Some(role))],
        ..Default::default()
    };

    assert_eq!(
        microphone(&state).len(),
        1,
        "Role.device names only one arbitrary stream's device; a second stream that IS a \
             real microphone must not be hidden by the first being a monitor"
    );
}

#[test]
fn volume_control_is_blocked_by_app_id() {
    let mut by_id = app("vc", "irrelevant", None, Some(role("mic", false)));
    by_id.app_id = Some("org.gnome.VolumeControl".to_owned());

    let state = AudioState {
        inputs: vec![device("mic")],
        apps: vec![by_id],
        ..Default::default()
    };

    assert!(microphone(&state).is_empty());
}

#[test]
fn a_hostile_name_claiming_to_be_the_volume_control_is_not_excluded() {
    let spoofed = app(
        "totally-not-spyware",
        "PulseAudio Volume Control",
        None,
        Some(role("mic", false)),
    );
    let state = AudioState {
        inputs: vec![device("mic")],
        apps: vec![spoofed],
        ..Default::default()
    };

    assert_eq!(
        microphone(&state).len(),
        1,
        "application.name is attacker-controlled; only the app id may gate the indicator"
    );
}

#[test]
fn the_application_name_is_preferred_and_its_input_suffix_is_stripped() {
    let state = AudioState {
        inputs: vec![device("mic")],
        apps: vec![app(
            "chrome",
            "Google Chrome input",
            Some("chrome"),
            Some(role("mic", false)),
        )],
        ..Default::default()
    };

    assert_eq!(
        microphone(&state)[0].app.as_deref(),
        Some("Google Chrome"),
        "the raw name is not a display name, but it is still the field to prefer once cleaned"
    );
}

#[test]
fn a_useless_binary_never_overrides_a_good_name() {
    let state = AudioState {
        inputs: vec![device("mic")],
        apps: vec![app(
            "walz",
            "walz",
            Some("WebKitWebProcess"),
            Some(role("mic", false)),
        )],
        ..Default::default()
    };

    assert_eq!(
        microphone(&state)[0].app.as_deref(),
        Some("walz"),
        "a WebKit-hosted app's own name is the useful one; its binary is not"
    );
}

#[test]
fn an_empty_name_and_no_binary_is_a_first_class_unnamed_row() {
    let state = AudioState {
        inputs: vec![device("mic")],
        apps: vec![app("7", "", None, Some(role("mic", false)))],
        ..Default::default()
    };

    assert_eq!(
        microphone(&state)[0].app,
        None,
        "a blank label is worse than an explicitly unnamed row"
    );
}

#[test]
fn a_four_kilobyte_app_name_is_capped_and_sanitized() {
    let long = "a".repeat(4096);
    let state = AudioState {
        inputs: vec![device("mic")],
        apps: vec![app("app", &long, None, Some(role("mic", false)))],
        ..Default::default()
    };

    let facts = microphone(&state);
    assert!(facts[0].app.as_ref().expect("a name").chars().count() <= NAME_CAP + 1);
}
