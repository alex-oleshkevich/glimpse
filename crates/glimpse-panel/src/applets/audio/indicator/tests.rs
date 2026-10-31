use super::*;
use crate::applet::runtime::HostInput;
use crate::applet::testing::{driven, run_in};
use glimpse_dbus::notifications::NotificationsProvider;
use glimpse_services::Running;

fn device(id: &str, volume: u32, muted: bool, default: bool) -> AudioDevice {
    AudioDevice {
        id: AudioDeviceId::new(id),
        index: 0,
        name: id.to_owned(),
        port: None,
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

fn started(audio: AudioHandle, notifications: NotificationsProviderHandle, volume: u32) -> Audio {
    let mut applet = Audio::start(audio, notifications);
    applet.state.outputs = vec![device("speakers", volume, false, true)];
    applet
}

#[tokio::test]
async fn several_notches_before_a_wake_add_up_and_clamp() {
    let (stopped, audio) = Running::<glimpse_services::Audio>::build(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    drop(stopped);
    let mut applet = started(
        audio.clone(),
        NotificationsProvider::unavailable("no bus in tests").handle(),
        50,
    );

    let targets: Vec<_> = (0..3)
        .map(|_| applet.adjust(Direction::Up).map(|(_, target)| target))
        .collect();
    assert_eq!(targets, [Some(55), Some(60), Some(65)]);

    applet.state.outputs[0].volume = 98;
    let targets: Vec<_> = (0..2)
        .map(|_| applet.adjust(Direction::Up).map(|(_, target)| target))
        .collect();
    assert_eq!(
        targets,
        [Some(100), None],
        "nothing is sent once the volume is already at 100"
    );
}

#[tokio::test]
async fn a_refused_volume_command_wakes_the_applet_and_the_next_wake_restores_the_chip() {
    let (stopped, audio) = Running::<glimpse_services::Audio>::build(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    drop(stopped);
    let (sender, inbox) = relm4::channel();
    let ctx = Ctx::new("audio".into(), None, sender);
    let mut applet = started(
        audio.clone(),
        NotificationsProvider::unavailable("no bus in tests").handle(),
        50,
    );
    let context = glib::MainContext::new();

    run_in(&context, || {
        for _ in 0..3 {
            applet.handle(&ctx, &Input::Pointer(Pointer::Scroll(Direction::Up)));
        }
    });
    assert_eq!(
        applet.state.default_output().map(|one| one.volume),
        Some(65)
    );

    assert!(matches!(
        driven(&context, inbox.recv()).await,
        Some(HostInput::Woken)
    ));
    applet.handle(&ctx, &Input::Woken);
    assert_eq!(applet.state, audio.snapshot());
}

fn snapshot(outputs: Vec<AudioDevice>) -> AudioState {
    AudioState {
        outputs,
        ..AudioState::default()
    }
}

fn idle(volume: u32) -> Audio {
    let (stopped, audio) = Running::<glimpse_services::Audio>::build(
        &glimpse_config::Config::default(),
        glimpse_dbus::Buses::unavailable("no bus in tests"),
    );
    drop(stopped);
    started(
        audio,
        NotificationsProvider::unavailable("no bus in tests").handle(),
        volume,
    )
}

fn volume(applet: &Audio) -> Option<u32> {
    applet.state.default_output().map(|one| one.volume)
}

#[tokio::test]
async fn a_stale_snapshot_between_notches_never_lowers_the_next_target() {
    let mut applet = idle(50);
    let now = Instant::now();
    let mut targets = Vec::new();

    for _ in 0..3 {
        targets.push(applet.adjust(Direction::Up).map(|(_, target)| target));
        applet.settle(snapshot(vec![device("speakers", 55, false, true)]), now);
    }

    assert_eq!(targets, [Some(55), Some(60), Some(65)]);
    assert_eq!(volume(&applet), Some(65));
}

#[tokio::test]
async fn a_snapshot_that_reaches_the_target_clears_the_pending_volume() {
    let mut applet = idle(50);
    let now = Instant::now();
    applet.adjust(Direction::Up);
    applet.adjust(Direction::Up);

    applet.settle(snapshot(vec![device("speakers", 60, false, true)]), now);
    assert!(applet.pending.borrow().is_none());

    applet.settle(snapshot(vec![device("speakers", 30, false, true)]), now);
    assert_eq!(volume(&applet), Some(30));
}

#[tokio::test]
async fn a_change_of_default_output_clears_the_pending_volume() {
    let mut applet = idle(50);
    let now = Instant::now();
    applet.adjust(Direction::Up);

    applet.settle(
        snapshot(vec![
            device("speakers", 50, false, false),
            device("headset", 40, false, true),
        ]),
        now,
    );

    assert!(applet.pending.borrow().is_none());
    assert_eq!(volume(&applet), Some(40));
}

#[tokio::test]
async fn a_pending_volume_expires_when_no_snapshot_reaches_it() {
    let mut applet = idle(50);
    applet.adjust(Direction::Up);

    applet.settle(
        snapshot(vec![device("speakers", 50, false, true)]),
        Instant::now() + PENDING_TTL,
    );

    assert!(applet.pending.borrow().is_none());
    assert_eq!(volume(&applet), Some(50));
}

fn app_volume(applet: &Audio, id: &str) -> Option<(u32, bool)> {
    applet
        .state
        .app(&AudioAppId::new(id))
        .and_then(|app| app.playback.as_ref())
        .map(|role| (role.volume, role.muted))
}

fn with_app(volume: u32, muted: bool) -> AudioState {
    AudioState {
        apps: vec![app(
            "firefox",
            Some(role("speakers", volume, muted, true)),
            None,
        )],
        ..AudioState::default()
    }
}

#[tokio::test]
async fn a_stale_snapshot_after_a_popover_slider_release_does_not_snap_back() {
    let mut applet = idle(40);
    let now = Instant::now();
    remember(
        &applet.pending,
        Target::Device(AudioDirection::Output, AudioDeviceId::new("speakers")),
        Some(70),
        None,
    );

    applet.settle(snapshot(vec![device("speakers", 40, false, true)]), now);
    assert_eq!(volume(&applet), Some(70));

    applet.settle(snapshot(vec![device("speakers", 70, false, true)]), now);
    assert_eq!(volume(&applet), Some(70));
    assert!(applet.pending.borrow().is_none());
}

#[tokio::test]
async fn a_stale_snapshot_after_an_input_slider_release_does_not_snap_back() {
    let mut applet = idle(40);
    let now = Instant::now();
    remember(
        &applet.pending,
        Target::Device(AudioDirection::Input, AudioDeviceId::new("mic")),
        Some(80),
        None,
    );
    let stale = AudioState {
        inputs: vec![device("mic", 20, false, true)],
        ..AudioState::default()
    };

    applet.settle(stale, now);

    assert_eq!(applet.state.default_input().map(|one| one.volume), Some(80));
}

#[tokio::test]
async fn two_quick_mute_clicks_end_unmuted_even_with_stale_snapshots() {
    let mut applet = idle(40);
    let target = || Target::Device(AudioDirection::Output, AudioDeviceId::new("speakers"));
    let stale = || snapshot(vec![device("speakers", 40, false, true)]);

    let first = flip(stale(), &applet.pending, target()).expect("the target exists");
    applet.settle(stale(), Instant::now());
    assert!(applet.state.default_output().is_some_and(|one| one.muted));
    let second = flip(stale(), &applet.pending, target()).expect("the target exists");
    applet.settle(stale(), Instant::now());

    assert_eq!((first, second), (true, false));
    assert!(applet.state.default_output().is_some_and(|one| !one.muted));
}

#[tokio::test]
async fn a_stale_snapshot_after_an_app_slider_release_does_not_snap_back() {
    let mut applet = idle(40);
    let now = Instant::now();
    remember(
        &applet.pending,
        Target::App(AudioAppId::new("firefox"), AudioDirection::Output),
        Some(90),
        None,
    );

    applet.settle(with_app(30, false), now);
    assert_eq!(app_volume(&applet, "firefox"), Some((90, false)));

    applet.settle(with_app(90, false), now);
    assert_eq!(app_volume(&applet, "firefox"), Some((90, false)));
    assert!(applet.pending.borrow().is_none());
}

#[tokio::test]
async fn two_quick_app_mute_clicks_end_unmuted() {
    let mut applet = idle(40);
    let target = || Target::App(AudioAppId::new("firefox"), AudioDirection::Output);

    let first = flip(with_app(30, false), &applet.pending, target()).expect("the target exists");
    applet.settle(with_app(30, false), Instant::now());
    assert_eq!(app_volume(&applet, "firefox"), Some((30, true)));
    let second = flip(with_app(30, false), &applet.pending, target()).expect("the target exists");
    applet.settle(with_app(30, false), Instant::now());

    assert_eq!((first, second), (true, false));
    assert_eq!(app_volume(&applet, "firefox"), Some((30, false)));
}

#[tokio::test]
async fn a_pending_app_override_clears_when_the_app_goes_away_or_expires() {
    let mut applet = idle(40);
    let target = || Target::App(AudioAppId::new("firefox"), AudioDirection::Output);
    remember(&applet.pending, target(), Some(90), None);
    applet.settle(AudioState::default(), Instant::now());
    assert!(applet.pending.borrow().is_none());

    remember(&applet.pending, target(), Some(90), None);
    applet.settle(with_app(30, false), Instant::now() + PENDING_TTL);
    assert!(applet.pending.borrow().is_none());
    assert_eq!(app_volume(&applet, "firefox"), Some((30, false)));
}
