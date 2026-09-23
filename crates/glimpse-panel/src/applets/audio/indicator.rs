use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gettextrs::{gettext, ngettext};
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    AUDIO_NAME_CAP, AudioApp, AudioAppId, AudioDevice, AudioDeviceId, AudioDirection, AudioError,
    AudioHandle, AudioRole, AudioState, CommandError,
};
use glimpse_widgets::{AudioBlock, AudioDetails, AudioEntry, AudioPopover, IndicatorSpec};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::{PopoverHandle, Seat, run};
use crate::applet::{
    Applet, Button, Ctx, Direction, Input, Opener, Pointer, Report, report_failure, spawn_reported,
};

use super::render;

const ICON: &str = "audio-volume-high-symbolic";
const APPS_SHOWN: usize = 6;
const SCROLL_STEP: i32 = 5;

pub struct Audio {
    state: AudioState,
    audio: AudioHandle,
    notifications: NotificationsProviderHandle,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    spec: Vec<IndicatorSpec>,
    expanded: Rc<Cell<bool>>,
    shown: glib::WeakRef<AudioPopover>,
}

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

impl Applet for Audio {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Audio {} = &config.kind else {
            return;
        };
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));
        self.refresh();
    }

    fn handle(&mut self, _ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => self.state = self.audio.snapshot(),
            Input::Pointer(Pointer::Scroll(direction)) => {
                self.nudge(*direction);
                return;
            }
            Input::Pointer(Pointer::Press(Button::Middle)) => {
                self.toggle_mute();
                return;
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = AudioPopover::new();

        shown.connect_level_changed({
            let audio = self.audio.clone();
            let notifications = self.notifications.clone();
            move |_, dir, value| {
                let Some(direction) = direction_of(dir) else {
                    return;
                };
                let audio = audio.clone();
                let snapshot = audio.snapshot();
                let Some(id) = default_device(&snapshot, direction).map(|device| device.id.clone())
                else {
                    return;
                };
                let percent = value.round().clamp(0.0, 100.0) as u32;
                tell(
                    &notifications,
                    "audio.set_device_volume",
                    gettext("Could not change the volume"),
                    async move { audio.set_device_volume(direction, id, percent).await },
                );
            }
        });

        shown.connect_level_toggled({
            let audio = self.audio.clone();
            let notifications = self.notifications.clone();
            move |_, dir, muted| {
                let Some(direction) = direction_of(dir) else {
                    return;
                };
                let audio = audio.clone();
                let snapshot = audio.snapshot();
                let Some(id) = default_device(&snapshot, direction).map(|device| device.id.clone())
                else {
                    return;
                };
                tell(
                    &notifications,
                    "audio.set_device_muted",
                    gettext("Could not change that setting"),
                    async move { audio.set_device_muted(direction, id, muted).await },
                );
            }
        });

        shown.connect_device_selected({
            let audio = self.audio.clone();
            let notifications = self.notifications.clone();
            let shown = shown.downgrade();
            let opener = seat.opener();
            move |_, dir, id| {
                let Some(direction) = direction_of(dir) else {
                    return;
                };
                let audio = audio.clone();
                let id = AudioDeviceId::new(id);
                act(
                    &notifications,
                    "audio.set_default",
                    gettext("Could not switch device"),
                    &shown,
                    &opener,
                    dir.to_owned(),
                    async move { audio.set_default(direction, id).await },
                );
            }
        });

        shown.connect_app_level_changed({
            let audio = self.audio.clone();
            let notifications = self.notifications.clone();
            move |_, app, dir, value| {
                let Some(direction) = direction_of(dir) else {
                    return;
                };
                let audio = audio.clone();
                let app = AudioAppId::new(app);
                let percent = value.round().clamp(0.0, 100.0) as u32;
                tell(
                    &notifications,
                    "audio.set_app_volume",
                    gettext("Could not change the volume"),
                    async move { audio.set_app_volume(direction, app, percent).await },
                );
            }
        });

        shown.connect_app_level_toggled({
            let audio = self.audio.clone();
            let notifications = self.notifications.clone();
            move |_, app, dir, muted| {
                let Some(direction) = direction_of(dir) else {
                    return;
                };
                let audio = audio.clone();
                let app = AudioAppId::new(app);
                tell(
                    &notifications,
                    "audio.set_app_muted",
                    gettext("Could not change that setting"),
                    async move { audio.set_app_muted(direction, app, muted).await },
                );
            }
        });

        shown.connect_app_moved({
            let audio = self.audio.clone();
            let notifications = self.notifications.clone();
            let shown = shown.downgrade();
            let opener = seat.opener();
            move |_, app, dir, device| {
                let Some(direction) = direction_of(dir) else {
                    return;
                };
                let audio = audio.clone();
                let key = app.to_owned();
                let app = AudioAppId::new(app);
                let device = AudioDeviceId::new(device);
                act(
                    &notifications,
                    "audio.move_app",
                    gettext("Could not move the application"),
                    &shown,
                    &opener,
                    key,
                    async move { audio.move_app(direction, app, device).await },
                );
            }
        });

        shown.connect_expanded({
            let expanded = Rc::clone(&self.expanded);
            let opener = seat.opener();
            move |_, place| {
                if place != "apps" {
                    return;
                }
                expanded.set(!expanded.get());
                opener.wake();
            }
        });

        if let Some((_, command)) = &self.footer {
            let command = command.clone();
            shown.connect_footer_activated(move |_| run(&command));
        }

        self.expanded.set(false);
        self.shown.set(Some(&shown));
        self.refresh();
        Some(Box::new(shown))
    }
}

fn tell<F, T>(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    summary: String,
    future: F,
) where
    F: std::future::Future<Output = Result<T, AudioError>> + Send + 'static,
    T: Send + 'static,
{
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Sound"),
        icon: ICON.to_owned(),
        summary,
    };
    spawn_reported(operation, report, wording, future);
}

#[allow(clippy::too_many_arguments)]
fn act(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    summary: String,
    shown: &glib::WeakRef<AudioPopover>,
    opener: &Opener,
    card: String,
    future: impl std::future::Future<Output = Result<(), AudioError>> + 'static,
) {
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Sound"),
        icon: ICON.to_owned(),
        summary,
    };
    let shown = shown.clone();
    let opener = opener.clone();
    relm4::spawn_local(async move {
        match future.await {
            Ok(()) => {
                if let Some(shown) = shown.upgrade() {
                    shown.collapse(&card);
                }
            }
            Err(error) => report_failure(operation, report, wording(&error), error).await,
        }
        opener.wake();
    });
}

fn wording(error: &AudioError) -> Option<String> {
    Some(match error {
        AudioError::Refused(_) => gettext("The audio server refused that."),
        AudioError::Unavailable => gettext("The audio server is unavailable."),
        AudioError::Service(error) => return service_wording(error),
    })
}

fn service_wording(error: &CommandError) -> Option<String> {
    Some(match error {
        CommandError::InvalidArgument(_) => gettext("That was not a valid value."),
        CommandError::Unavailable(_) => gettext("The audio server is unavailable."),
        CommandError::Unsupported(_) => gettext("That is not supported."),
        CommandError::LimitExceeded(_) => gettext("That could not be completed."),
        CommandError::Internal(_) => gettext("That did not work."),
    })
}

fn direction_of(dir: &str) -> Option<AudioDirection> {
    match dir {
        "output" => Some(AudioDirection::Output),
        "input" => Some(AudioDirection::Input),
        _ => None,
    }
}

fn dir_key(dir: AudioDirection) -> &'static str {
    match dir {
        AudioDirection::Output => "output",
        AudioDirection::Input => "input",
    }
}

fn direction_label(dir: AudioDirection) -> String {
    match dir {
        AudioDirection::Output => gettext("Output"),
        AudioDirection::Input => gettext("Input"),
    }
}

fn default_device(state: &AudioState, dir: AudioDirection) -> Option<&AudioDevice> {
    match dir {
        AudioDirection::Output => state.default_output(),
        AudioDirection::Input => state.default_input(),
    }
}

fn shown_count(cap: usize, expanded: bool) -> usize {
    match expanded {
        true => usize::MAX,
        false => cap,
    }
}

fn more_apps(total: usize, cap: usize, expanded: bool) -> Option<String> {
    let hidden = total.saturating_sub(cap);
    if hidden == 0 {
        return None;
    }
    Some(match expanded {
        true => gettext("Show fewer"),
        false => ngettext("{count} more app", "{count} more apps", hidden as u32)
            .replace("{count}", &hidden.to_string()),
    })
}

fn device_entry(device: &AudioDevice, dir: AudioDirection) -> AudioEntry {
    AudioEntry {
        id: device.id.as_str().to_owned(),
        title: render::cap(&device.name, AUDIO_NAME_CAP),
        icon: Some(render::device_icon(device, dir).to_owned()),
        value: None,
        selected: device.default,
        muted_icon: None,
    }
}

/// Every device of one direction, the default first: they live in the card under the current
/// device's row, which is only opened to switch, so nothing caps them.
fn device_entries(devices: &[AudioDevice], dir: AudioDirection) -> Vec<AudioEntry> {
    let mut ordered: Vec<&AudioDevice> = devices.iter().collect();
    ordered.sort_by_key(|device| !device.default);
    ordered
        .into_iter()
        .map(|device| device_entry(device, dir))
        .collect()
}

/// A muted app carries the same muted glyph its fader would — a speaker for playback, a
/// microphone for an app that only records — which the popover draws in the warning colour.
fn muted_glyph(app: &AudioApp) -> Option<&'static str> {
    let (direction, role) = match (&app.playback, &app.capture) {
        (Some(role), _) => (AudioDirection::Output, role),
        (None, Some(role)) => (AudioDirection::Input, role),
        (None, None) => return None,
    };
    role.muted
        .then(|| render::fader_icon(direction, role.volume, true))
}

fn app_entry(app: &AudioApp) -> AudioEntry {
    AudioEntry {
        id: app.id.as_str().to_owned(),
        title: render::app_name(app),
        icon: Some(render::app_icon(app)),
        value: None,
        selected: false,
        muted_icon: muted_glyph(app).map(str::to_owned),
    }
}

fn app_entries(apps: &[AudioApp], cap: usize, expanded: bool) -> Vec<AudioEntry> {
    apps.iter()
        .take(shown_count(cap, expanded))
        .map(app_entry)
        .collect()
}

fn block(
    state: &AudioState,
    dir: AudioDirection,
    role: &AudioRole,
    with_heading: bool,
) -> AudioBlock {
    AudioBlock {
        dir: dir_key(dir).to_owned(),
        heading: with_heading.then(|| direction_label(dir)),
        volume: role.volume as f64,
        muted: role.muted,
        icon: Some(render::fader_icon(dir, role.volume, role.muted).to_owned()),
        adjustable: role.adjustable,
        devices: state
            .devices(dir)
            .iter()
            .map(|device| AudioEntry {
                id: device.id.as_str().to_owned(),
                title: render::cap(&device.name, AUDIO_NAME_CAP),
                icon: Some(render::device_icon(device, dir).to_owned()),
                value: None,
                selected: device.id == role.device,
                muted_icon: None,
            })
            .collect(),
    }
}

fn details(state: &AudioState, id: &AudioAppId) -> Option<AudioDetails> {
    let app = state.app(id)?;
    let both = app.playback.is_some() && app.capture.is_some();
    let mut blocks = Vec::new();
    if let Some(role) = &app.playback {
        blocks.push(block(state, AudioDirection::Output, role, both));
    }
    if let Some(role) = &app.capture {
        blocks.push(block(state, AudioDirection::Input, role, both));
    }
    Some(AudioDetails {
        id: id.as_str().to_owned(),
        blocks,
    })
}

impl Audio {
    fn nudge(&self, direction: Direction) {
        let Some(device) = self.state.default_output() else {
            return;
        };
        let step = match direction {
            Direction::Up | Direction::Right => SCROLL_STEP,
            Direction::Down | Direction::Left => -SCROLL_STEP,
        };
        let target = (device.volume as i32 + step).clamp(0, 100) as u32;
        if target == device.volume {
            return;
        }
        let audio = self.audio.clone();
        let id = device.id.clone();
        tell(
            &self.notifications,
            "audio.set_volume",
            gettext("Could not change the volume"),
            async move {
                audio
                    .set_device_volume(AudioDirection::Output, id, target)
                    .await
            },
        );
    }

    fn toggle_mute(&self) {
        let Some(device) = self.state.default_output() else {
            return;
        };
        let audio = self.audio.clone();
        let id = device.id.clone();
        let muted = !device.muted;
        tell(
            &self.notifications,
            "audio.set_device_muted",
            gettext("Could not change that setting"),
            async move {
                audio
                    .set_device_muted(AudioDirection::Output, id, muted)
                    .await
            },
        );
    }

    pub fn start(audio: AudioHandle, notifications: NotificationsProviderHandle) -> Self {
        let state = audio.snapshot();
        Self {
            state,
            audio,
            notifications,
            tooltip_format: None,
            footer: None,
            spec: Vec::new(),
            expanded: Rc::new(Cell::new(false)),
            shown: glib::WeakRef::new(),
        }
    }

    fn refresh(&mut self) {
        self.spec = self.indicator().into_iter().collect();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &AudioPopover) {
        let output = self.state.default_output();
        let input = self.state.default_input();

        shown.set_heading(
            render::fader_icon(
                AudioDirection::Output,
                output.map_or(0, |device| device.volume),
                output.is_none_or(|device| device.muted),
            ),
            output.map(|device| device.name.as_str()),
            output.is_some_and(|device| device.muted),
        );

        shown.set_output_level(
            output.map_or(0.0, |device| device.volume as f64),
            output.is_some_and(|device| device.muted),
            output.map(|device| {
                render::fader_icon(AudioDirection::Output, device.volume, device.muted)
            }),
        );
        shown.set_input_level(
            input.map_or(0.0, |device| device.volume as f64),
            input.is_some_and(|device| device.muted),
            input.map(|device| {
                render::fader_icon(AudioDirection::Input, device.volume, device.muted)
            }),
        );

        shown.set_outputs(&device_entries(&self.state.outputs, AudioDirection::Output));
        shown.set_inputs(&device_entries(&self.state.inputs, AudioDirection::Input));

        let apps_expanded = self.expanded.get();
        shown.set_apps(&app_entries(&self.state.apps, APPS_SHOWN, apps_expanded));
        shown.set_more_apps(more_apps(self.state.apps.len(), APPS_SHOWN, apps_expanded).as_deref());
        let details: Vec<_> = self
            .state
            .apps
            .iter()
            .filter_map(|app| details(&self.state, &app.id))
            .collect();
        shown.set_details(&details);
        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));
    }

    fn indicator(&self) -> Option<IndicatorSpec> {
        Some(IndicatorSpec {
            icon: Some(themed(render::chip(&self.state)?)),
            overlay: render::overlay(&self.state).map(themed),
            tooltip: render::tooltip(&self.state, self.tooltip_format.as_deref()),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
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
}
