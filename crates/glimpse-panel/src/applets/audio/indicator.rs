use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gettextrs::{gettext, ngettext};
use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    AUDIO_NAME_CAP, AudioApp, AudioAppId, AudioDevice, AudioDeviceId, AudioDirection, AudioError,
    AudioHandle, AudioRole, AudioState, CommandError,
};
use glimpse_widgets::{AudioBlock, AudioDetails, AudioEntry, AudioPopover, IndicatorSpec};
use gtk4::glib;
use gtk4::prelude::*;

use crate::applet::popover::Seat;
use crate::applet::themed;
use crate::applet::{
    Applet, Button, Ctx, Direction, Input, Opener, Pointer, Report, report_failure,
};

use super::render;

const ICON: &str = "audio-volume-high-symbolic";
const APPS_SHOWN: usize = 6;
const PENDING_TTL: Duration = Duration::from_millis(500);

#[derive(PartialEq)]
enum Target {
    Device(AudioDirection, AudioDeviceId),
    App(AudioAppId, AudioDirection),
}

struct Pending {
    target: Target,
    volume: Option<u32>,
    muted: Option<bool>,
    at: Instant,
}

pub struct Audio {
    state: AudioState,
    audio: AudioHandle,
    pending: Rc<RefCell<Option<Pending>>>,
    notifications: NotificationsProviderHandle,
    tooltip_format: Option<String>,
    scroll_step: i32,
    spec: Vec<IndicatorSpec>,
    expanded: Rc<Cell<bool>>,
    shown: glib::WeakRef<AudioPopover>,
}

impl Applet for Audio {
    fn configure(&mut self, _ctx: &Ctx, config: &AppletConfig) {
        let AppletKind::Audio(settings) = &config.kind else {
            return;
        };
        self.scroll_step = i32::from(settings.scroll_step);
        self.tooltip_format = config.kind.tooltip_format().map(str::to_owned);

        self.refresh();
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => self.settle(self.audio.snapshot(), Instant::now()),
            Input::Pointer(Pointer::Scroll(direction)) => self.nudge(ctx, *direction),
            Input::Pointer(Pointer::Press(Button::Middle)) => self.toggle_mute(ctx),
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<gtk4::Widget> {
        let shown = AudioPopover::new();

        shown.connect_level_changed({
            let audio = self.audio.clone();
            let pending = Rc::clone(&self.pending);
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            move |_, dir, value| {
                let Some(direction) = direction_of(dir) else {
                    return;
                };
                let audio = audio.clone();
                let state = effective(&audio, &pending);
                let Some(id) = default_device(&state, direction).map(|device| device.id.clone())
                else {
                    return;
                };
                let percent = value.round().clamp(0.0, 100.0) as u32;
                remember(
                    &pending,
                    Target::Device(direction, id.clone()),
                    Some(percent),
                    None,
                );
                opener.wake();
                tell(
                    &notifications,
                    &opener,
                    &pending,
                    "audio.set_device_volume",
                    gettext("Could not change the volume"),
                    async move { audio.set_device_volume(direction, id, percent).await },
                );
            }
        });

        shown.connect_level_toggled({
            let audio = self.audio.clone();
            let pending = Rc::clone(&self.pending);
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            move |_, dir, _| {
                let Some(direction) = direction_of(dir) else {
                    return;
                };
                let audio = audio.clone();
                let snapshot = audio.snapshot();
                let Some(id) = default_device(&snapshot, direction).map(|device| device.id.clone())
                else {
                    return;
                };
                let Some(muted) = flip(snapshot, &pending, Target::Device(direction, id.clone()))
                else {
                    return;
                };
                opener.wake();
                tell(
                    &notifications,
                    &opener,
                    &pending,
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
            let pending = Rc::clone(&self.pending);
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            move |_, app, dir, value| {
                let Some(direction) = direction_of(dir) else {
                    return;
                };
                let audio = audio.clone();
                let app = AudioAppId::new(app);
                let percent = value.round().clamp(0.0, 100.0) as u32;
                remember(
                    &pending,
                    Target::App(app.clone(), direction),
                    Some(percent),
                    None,
                );
                opener.wake();
                tell(
                    &notifications,
                    &opener,
                    &pending,
                    "audio.set_app_volume",
                    gettext("Could not change the volume"),
                    async move { audio.set_app_volume(direction, app, percent).await },
                );
            }
        });

        shown.connect_app_level_toggled({
            let audio = self.audio.clone();
            let pending = Rc::clone(&self.pending);
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            move |_, app, dir, _| {
                let Some(direction) = direction_of(dir) else {
                    return;
                };
                let audio = audio.clone();
                let app = AudioAppId::new(app);
                let Some(muted) = flip(
                    audio.snapshot(),
                    &pending,
                    Target::App(app.clone(), direction),
                ) else {
                    return;
                };
                opener.wake();
                tell(
                    &notifications,
                    &opener,
                    &pending,
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

        self.expanded.set(false);
        self.shown.set(Some(&shown));
        self.refresh();
        Some(shown.upcast())
    }
}

fn tell<F, T>(
    notifications: &NotificationsProviderHandle,
    opener: &Opener,
    pending: &Rc<RefCell<Option<Pending>>>,
    operation: &'static str,
    summary: String,
    future: F,
) where
    F: std::future::Future<Output = Result<T, AudioError>> + 'static,
    T: 'static,
{
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Sound"),
        icon: ICON.to_owned(),
        summary,
    };
    let opener = opener.clone();
    let pending = Rc::clone(pending);
    relm4::spawn_local(async move {
        if let Err(error) = future.await {
            *pending.borrow_mut() = None;
            opener.wake();
            report_failure(operation, report, wording(&error), error).await;
        }
    });
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

pub(crate) fn wording(error: &AudioError) -> Option<String> {
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
        title: glimpse_utils::clean(&device.name, AUDIO_NAME_CAP),
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
                title: glimpse_utils::clean(&device.name, AUDIO_NAME_CAP),
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

fn fields<'a>(state: &'a mut AudioState, target: &Target) -> Option<(&'a mut u32, &'a mut bool)> {
    match target {
        Target::Device(direction, id) => {
            let devices = match direction {
                AudioDirection::Output => &mut state.outputs,
                AudioDirection::Input => &mut state.inputs,
            };
            devices
                .iter_mut()
                .find(|device| device.default && device.id == *id)
                .map(|device| (&mut device.volume, &mut device.muted))
        }
        Target::App(id, direction) => state
            .apps
            .iter_mut()
            .find(|app| app.id == *id)
            .and_then(|app| match direction {
                AudioDirection::Output => app.playback.as_mut(),
                AudioDirection::Input => app.capture.as_mut(),
            })
            .map(|role| (&mut role.volume, &mut role.muted)),
    }
}

fn apply(state: &mut AudioState, slot: &mut Option<Pending>, now: Instant) {
    let Some(pending) = slot.as_mut() else {
        return;
    };
    let Some((volume, muted)) = fields(state, &pending.target) else {
        *slot = None;
        return;
    };
    if now.saturating_duration_since(pending.at) >= PENDING_TTL {
        *slot = None;
        return;
    }
    if pending.volume == Some(*volume) {
        pending.volume = None;
    }
    if pending.muted == Some(*muted) {
        pending.muted = None;
    }
    if pending.volume.is_none() && pending.muted.is_none() {
        *slot = None;
        return;
    }
    if let Some(target) = pending.volume {
        *volume = target;
    }
    if let Some(target) = pending.muted {
        *muted = target;
    }
}

fn remember(
    slot: &RefCell<Option<Pending>>,
    target: Target,
    volume: Option<u32>,
    muted: Option<bool>,
) {
    let mut slot = slot.borrow_mut();
    let at = Instant::now();
    match slot.as_mut() {
        Some(pending) if pending.target == target => {
            if volume.is_some() {
                pending.volume = volume;
            }
            if muted.is_some() {
                pending.muted = muted;
            }
            pending.at = at;
        }
        _ => {
            *slot = Some(Pending {
                target,
                volume,
                muted,
                at,
            })
        }
    }
}

fn effective(audio: &AudioHandle, slot: &RefCell<Option<Pending>>) -> AudioState {
    let mut state = audio.snapshot();
    apply(&mut state, &mut slot.borrow_mut(), Instant::now());
    state
}

fn flip(mut state: AudioState, slot: &RefCell<Option<Pending>>, target: Target) -> Option<bool> {
    apply(&mut state, &mut slot.borrow_mut(), Instant::now());
    let muted = !*fields(&mut state, &target)?.1;
    remember(slot, target, None, Some(muted));
    Some(muted)
}

impl Audio {
    fn settle(&mut self, snapshot: AudioState, now: Instant) {
        self.state = snapshot;
        apply(&mut self.state, &mut self.pending.borrow_mut(), now);
    }

    fn adjust(&mut self, direction: Direction) -> Option<(AudioDeviceId, u32)> {
        let step = match direction {
            Direction::Up | Direction::Right => self.scroll_step,
            Direction::Down | Direction::Left => -self.scroll_step,
        };
        let device = self
            .state
            .outputs
            .iter_mut()
            .find(|device| device.default)?;
        let target = (device.volume as i32 + step).clamp(0, 100) as u32;
        if target == device.volume {
            return None;
        }
        device.volume = target;
        let id = device.id.clone();
        remember(
            &self.pending,
            Target::Device(AudioDirection::Output, id.clone()),
            Some(target),
            None,
        );
        Some((id, target))
    }

    fn nudge(&mut self, ctx: &Ctx, direction: Direction) {
        let Some((id, target)) = self.adjust(direction) else {
            return;
        };
        let audio = self.audio.clone();
        tell(
            &self.notifications,
            &ctx.opener(),
            &self.pending,
            "audio.set_volume",
            gettext("Could not change the volume"),
            async move {
                audio
                    .set_device_volume(AudioDirection::Output, id, target)
                    .await
            },
        );
    }

    fn toggle_mute(&mut self, ctx: &Ctx) {
        let Some(device) = self.state.outputs.iter_mut().find(|device| device.default) else {
            return;
        };
        let audio = self.audio.clone();
        let id = device.id.clone();
        device.muted = !device.muted;
        let muted = device.muted;
        remember(
            &self.pending,
            Target::Device(AudioDirection::Output, id.clone()),
            None,
            Some(muted),
        );
        tell(
            &self.notifications,
            &ctx.opener(),
            &self.pending,
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
            pending: Rc::new(RefCell::new(None)),
            notifications,
            tooltip_format: None,
            scroll_step: 5,
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
            output
                .map(|device| glimpse_utils::clean(&device.name, AUDIO_NAME_CAP))
                .as_deref(),
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
mod tests;
