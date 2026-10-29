use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

use glimpse_widgets::{
    ChipGroup, LockStage, MessageKind, PasswordPrompt, SessionActionState, Track, TransportAction,
};
use gtk4::glib::SignalHandlerId;
use gtk4::prelude::*;
use gtk4::{gdk, glib};
use gtk4_session_lock::Instance;
use zeroize::Zeroizing;

use crate::lifecycle::Input;
use crate::session::{ACTIONS, Action, Outcome};
use crate::status::Status;

type Report = Rc<dyn Fn(Input)>;

pub const FOCUSED: &str = "focused";

pub struct Look {
    pub backgrounds: HashMap<String, gdk::Texture>,
    pub color: gdk::RGBA,
    pub fit: gtk4::ContentFit,
    pub dim: f64,
    pub clock: (String, String),
    pub user: Option<String>,
    pub avatar: Option<gdk::Texture>,
    pub prompt_output: String,
    pub session: HashMap<&'static str, SessionActionState>,
    pub track: Option<Track>,
    pub notifications: Vec<ChipGroup>,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptState {
    pub available: bool,
    pub busy: bool,
    pub message: Option<String>,
    pub lasting: bool,
}

impl Default for PromptState {
    fn default() -> Self {
        Self {
            available: true,
            busy: false,
            message: None,
            lasting: false,
        }
    }
}

pub struct Surfaces {
    instance: Instance,
    shared: Rc<Shared>,
    caps: Option<SignalHandlerId>,
}

struct Shared {
    generation: u64,
    focused: RefCell<Option<String>>,
    look: RefCell<Look>,
    prompt: RefCell<PromptState>,
    keyboard: Option<gdk::Device>,
    stages: RefCell<Vec<Stage>>,
    interactive: RefCell<Option<gdk::Monitor>>,
    submitter: RefCell<Option<gdk::Monitor>>,
    in_flight: RefCell<Option<(u64, gdk::Monitor)>>,
    request_ids: Rc<Cell<u64>>,
    clock: Timer,
    unpainted: Cell<usize>,
    report: Report,
    on_session_action: Rc<dyn Fn(Action, u64)>,
    on_track_action: Rc<dyn Fn(TransportAction)>,
}

struct Stage {
    monitor: gdk::Monitor,
    window: gtk4::Window,
    stage: LockStage,
    painted: Rc<Cell<bool>>,
    invalidate: Option<SignalHandlerId>,
}

impl Drop for Stage {
    fn drop(&mut self) {
        if let Some(handler) = self.invalidate.take() {
            self.monitor.disconnect(handler);
        }
    }
}

pub fn prompt_stage(
    connectors: &[String],
    focused: Option<&str>,
    active: Option<usize>,
    current: Option<usize>,
    fallback: bool,
) -> Option<usize> {
    if let Some(active) = active.filter(|&active| active < connectors.len()) {
        return (current != Some(active)).then_some(active);
    }
    let focused = focused.and_then(|focused| connectors.iter().position(|c| c == focused));
    match (focused, current) {
        (Some(index), current) => (current != Some(index)).then_some(index),
        (None, None) if fallback && !connectors.is_empty() => Some(0),
        (None, _) => None,
    }
}

pub fn prompt_output<'a>(configured: &'a str, focused: Option<&'a str>) -> Option<&'a str> {
    match configured {
        FOCUSED => focused,
        connector => Some(connector),
    }
}

pub fn accepts<T: PartialEq>(submitter: Option<&T>, interactive: Option<&T>) -> bool {
    submitter.is_some() && submitter == interactive
}

pub fn carried_awake(previous: Option<bool>) -> bool {
    previous.unwrap_or_default()
}

pub fn submitted(text: Zeroizing<String>) -> Option<Zeroizing<String>> {
    (!text.is_empty()).then_some(text)
}

pub fn session_result_target<'a>(
    requester: Option<&'a str>,
    interactive: Option<&'a str>,
    connectors: &[String],
) -> Option<&'a str> {
    requester
        .filter(|candidate| connectors.iter().any(|c| c == candidate))
        .or_else(|| interactive.filter(|candidate| connectors.iter().any(|c| c == candidate)))
}

#[derive(Default)]
struct Timer(RefCell<Option<glib::SourceId>>);

impl Timer {
    fn arm(&self, delay: Duration, fire: impl FnOnce() + 'static) {
        self.cancel();
        self.0
            .replace(Some(glib::timeout_add_local_once(delay, fire)));
    }

    fn fired(&self) {
        self.0.take();
    }

    fn cancel(&self) {
        if let Some(source) = self.0.take() {
            source.remove();
        }
    }
}

pub fn next_minute(seconds: f64) -> Duration {
    let into = match seconds.is_finite() {
        true => seconds.rem_euclid(60.0),
        false => 0.0,
    };
    let millis = ((60.0 - into) * 1000.0).ceil() as u64;
    Duration::from_millis(millis.clamp(1, 60_000))
}

impl Surfaces {
    pub fn lock(
        generation: u64,
        look: Look,
        focused: Option<&str>,
        request_ids: Rc<Cell<u64>>,
        report: impl Fn(Input) + 'static,
        on_session_action: impl Fn(Action, u64) + 'static,
        on_track_action: impl Fn(TransportAction) + 'static,
    ) -> Self {
        let report: Report = Rc::new(report);
        let instance = Instance::new();
        let keyboard = gdk::Display::default()
            .and_then(|display| display.default_seat())
            .and_then(|seat| seat.keyboard());
        let shared = Rc::new(Shared {
            generation,
            focused: RefCell::new(focused.map(str::to_owned)),
            look: RefCell::new(look),
            prompt: RefCell::default(),
            keyboard,
            stages: RefCell::default(),
            interactive: RefCell::default(),
            submitter: RefCell::default(),
            in_flight: RefCell::default(),
            request_ids,
            clock: Timer::default(),
            unpainted: Cell::new(0),
            report: report.clone(),
            on_session_action: Rc::new(on_session_action),
            on_track_action: Rc::new(on_track_action),
        });

        let failed = Rc::new(Cell::new(false));
        instance.connect_locked({
            let report = report.clone();
            move |_| report(Input::Locked(generation))
        });
        instance.connect_failed({
            let report = report.clone();
            let failed = failed.clone();
            move |_| {
                failed.set(true);
                report(Input::Failed(generation));
            }
        });
        instance.connect_unlocked({
            let report = report.clone();
            move |_| report(Input::Unlocked(generation))
        });

        let caps = shared.keyboard.as_ref().map(|keyboard| {
            let shared = Rc::downgrade(&shared);
            keyboard.connect_caps_lock_state_notify(move |keyboard| {
                if let Some(shared) = shared.upgrade() {
                    shared.each_prompt(|prompt| prompt.set_caps_lock(keyboard.is_caps_locked()));
                }
            })
        });
        report(Input::Unpainted(generation));
        if !instance.lock() {
            report(Input::Failed(generation));
        } else if let Some(display) = gdk::Display::default() {
            display.sync();
            if !failed.get() {
                instance.connect_monitor({
                    let shared = Rc::downgrade(&shared);
                    move |instance, monitor| {
                        if let Some(shared) = shared.upgrade() {
                            Shared::add(&shared, instance, monitor);
                        }
                    }
                });
                let monitors = display.monitors();
                for index in 0..monitors.n_items() {
                    if let Some(monitor) = monitors.item(index).and_downcast::<gdk::Monitor>() {
                        Shared::add(&shared, &instance, &monitor);
                    }
                }
            }
        }
        shared.ensure_interactive();
        arm_clock(&shared);
        Self {
            instance,
            shared,
            caps,
        }
    }

    pub fn render(&self, state: PromptState) {
        self.shared.ensure_interactive();
        let available = state.available;
        let changed = *self.shared.prompt.borrow() != state;
        self.shared.prompt.replace(state);
        for stage in self.shared.stages.borrow().iter() {
            self.shared.apply_prompt(&stage.stage);
        }
        self.shared.tick();
        if changed
            && available
            && let Some(stage) = self.shared.interactive_stage()
        {
            focus_prompt(&stage);
        }
    }

    pub fn set_look(&self, look: Look) {
        let moved = look.prompt_output != self.shared.look.borrow().prompt_output;
        self.shared.look.replace(look);
        for stage in self.shared.stages.borrow().iter() {
            self.shared.apply_look(stage);
        }
        if moved {
            self.shared.place(false);
        }
    }

    pub fn resumed(&self) {
        for stage in self.shared.stages.borrow().iter() {
            stage.stage.set_awake(false);
        }
        self.shared.tick();
        arm_clock(&self.shared);
    }

    pub fn place_prompt(&self) {
        self.shared.place(false);
    }

    pub fn set_focused(&self, output: Option<&str>) {
        self.shared.focused.replace(output.map(str::to_owned));
        self.shared.place(false);
    }

    pub fn shake(&self) {
        for stage in self.shared.stages.borrow().iter() {
            stage.stage.set_awake(true);
        }
        self.shared.each_prompt(PasswordPrompt::shake);
    }

    pub fn take_text(&self) -> Option<Zeroizing<String>> {
        let submitter = self.shared.submitter.take();
        let accepted = accepts(
            submitter.as_ref(),
            self.shared.interactive.borrow().as_ref(),
        );
        let stage = self.shared.interactive_stage().filter(|_| accepted)?;
        submitted(stage.prompt().take_text())
    }

    pub fn discard_submit(&self) {
        self.shared.submitter.take();
    }

    pub fn unlock(&self) {
        self.instance.unlock();
    }

    pub fn session_action_result(&self, id: u64, action: Action, outcome: Outcome) {
        let requester = {
            let mut in_flight = self.shared.in_flight.borrow_mut();
            let current = in_flight.as_ref().map(|(in_flight_id, _)| *in_flight_id);
            if current != Some(id) {
                tracing::debug!(
                    id,
                    in_flight = current,
                    action = action.key(),
                    "a session action result for a request no longer in flight"
                );
                return;
            }
            in_flight.take().map(|(_, monitor)| monitor)
        };
        match outcome {
            Outcome::Succeeded => {
                for stage in self.shared.stages.borrow().iter() {
                    stage.stage.close_session();
                }
            }
            Outcome::TimedOut => {
                tracing::warn!(
                    id,
                    action = action.key(),
                    "logind did not answer in time; the action may still happen"
                );
            }
            Outcome::Failed => match self.shared.session_result_stage(requester.as_ref()) {
                Some(stage) => stage.set_session_error(Some(&action.error_text())),
                None => tracing::warn!(
                    id,
                    action = action.key(),
                    "a session action failed after its sheet closed"
                ),
            },
        }
    }
}

impl Drop for Surfaces {
    fn drop(&mut self) {
        self.shared.clock.cancel();
        if let (Some(keyboard), Some(handler)) = (&self.shared.keyboard, self.caps.take()) {
            keyboard.disconnect(handler);
        }
    }
}

impl Shared {
    fn add(shared: &Rc<Self>, instance: &Instance, monitor: &gdk::Monitor) {
        let (window, stage) = build();
        let painted = Rc::new(Cell::new(false));
        let unpainted = shared.unpainted.get();
        shared.unpainted.set(unpainted + 1);
        if unpainted == 0 {
            (shared.report)(Input::Unpainted(shared.generation));
        }
        window.connect_map({
            let shared = Rc::downgrade(shared);
            let painted = painted.clone();
            move |window| on_first_paint(window, shared.clone(), painted.clone())
        });
        instance.assign_window_to_monitor(&window, monitor);
        if window.is_mapped() {
            on_first_paint(&window, Rc::downgrade(shared), painted.clone());
        }
        let invalidate = monitor.connect_invalidate({
            let shared = Rc::downgrade(shared);
            move |monitor| {
                if let Some(shared) = shared.upgrade() {
                    shared.remove(monitor);
                }
            }
        });

        window.connect_is_active_notify({
            let shared = Rc::downgrade(shared);
            move |window| {
                if window.is_active()
                    && let Some(shared) = shared.upgrade()
                {
                    shared.place(false);
                }
            }
        });

        stage.set_interactive(false);
        stage.prompt().connect_submitted({
            let shared = Rc::downgrade(shared);
            let monitor = monitor.clone();
            move |_| {
                if let Some(shared) = shared.upgrade()
                    && shared.interactive.borrow().as_ref() == Some(&monitor)
                {
                    shared.submitter.replace(Some(monitor.clone()));
                    (shared.report)(Input::Submit);
                }
            }
        });
        stage.connect_session_action({
            let shared = Rc::downgrade(shared);
            let monitor = monitor.clone();
            move |_, action| {
                let Some(shared) = shared.upgrade() else {
                    return;
                };
                if !accepts(Some(&monitor), shared.interactive.borrow().as_ref()) {
                    return;
                }
                let Some(action) = Action::from_key(action) else {
                    tracing::warn!(action, "unknown session action requested");
                    return;
                };
                if shared.in_flight.borrow().is_some() {
                    tracing::debug!(
                        action = action.key(),
                        "ignoring a session action while one is already in flight"
                    );
                    return;
                }
                let id = shared.request_ids.get();
                shared.request_ids.set(id + 1);
                shared.in_flight.replace(Some((id, monitor.clone())));
                (shared.on_session_action)(action, id);
            }
        });
        stage.track().connect_action({
            let shared = Rc::downgrade(shared);
            let monitor = monitor.clone();
            move |_, action| {
                if let Some(shared) = shared.upgrade()
                    && accepts(Some(&monitor), shared.interactive.borrow().as_ref())
                {
                    (shared.on_track_action)(action);
                }
            }
        });
        if let Some(keyboard) = &shared.keyboard {
            stage.prompt().set_caps_lock(keyboard.is_caps_locked());
            stage.prompt().connect_map({
                let keyboard = keyboard.clone();
                move |prompt| prompt.set_caps_lock(keyboard.is_caps_locked())
            });
        }

        let added = Stage {
            monitor: monitor.clone(),
            window: window.clone(),
            stage,
            painted,
            invalidate: Some(invalidate),
        };
        shared.apply_look(&added);
        shared.apply_prompt(&added.stage);
        shared.stages.borrow_mut().push(added);
        shared.place(false);
        shared.ensure_interactive();
    }

    fn remove(&self, monitor: &gdk::Monitor) {
        let removed = {
            let mut stages = self.stages.borrow_mut();
            let index = stages.iter().position(|stage| &stage.monitor == monitor);
            index.map(|index| stages.remove(index))
        };
        let Some(stage) = removed else {
            return;
        };
        if !stage.painted.replace(true) {
            self.paint_done();
        }
        if self.interactive.borrow().as_ref() == Some(monitor) {
            self.interactive.replace(None);
        }
        self.ensure_interactive();
    }

    fn interactive_stage(&self) -> Option<LockStage> {
        let interactive = self.interactive.borrow();
        let monitor = interactive.as_ref()?;
        self.stages
            .borrow()
            .iter()
            .find(|stage| &stage.monitor == monitor)
            .map(|stage| stage.stage.clone())
    }

    fn ensure_interactive(&self) {
        if self.interactive_stage().is_none() {
            self.place(true);
        }
    }

    fn session_result_stage(&self, requester: Option<&gdk::Monitor>) -> Option<LockStage> {
        let stages = self.stages.borrow();
        let connector =
            |monitor: &gdk::Monitor| monitor.connector().map(String::from).unwrap_or_default();
        let connectors: Vec<String> = stages
            .iter()
            .map(|stage| connector(&stage.monitor))
            .collect();
        let requester = requester.map(connector);
        let interactive = self.interactive.borrow();
        let interactive = interactive.as_ref().map(connector);
        let target =
            session_result_target(requester.as_deref(), interactive.as_deref(), &connectors)?;
        stages
            .iter()
            .find(|stage| connector(&stage.monitor) == target)
            .map(|stage| stage.stage.clone())
    }

    fn place(&self, fallback: bool) {
        let index = {
            let stages = self.stages.borrow();
            let connectors = stages
                .iter()
                .map(|stage| {
                    stage
                        .monitor
                        .connector()
                        .map(String::from)
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>();
            let interactive = self.interactive.borrow();
            let current = stages
                .iter()
                .position(|stage| interactive.as_ref() == Some(&stage.monitor));
            let active = stages.iter().position(|stage| stage.window.is_active());
            let look = self.look.borrow();
            let focused = self.focused.borrow();
            prompt_stage(
                &connectors,
                prompt_output(&look.prompt_output, focused.as_deref()),
                active,
                current,
                fallback,
            )
        };
        if let Some(index) = index {
            self.make_interactive(index);
        }
    }

    fn make_interactive(&self, index: usize) {
        let Some((monitor, stage)) = self
            .stages
            .borrow()
            .get(index)
            .map(|stage| (stage.monitor.clone(), stage.stage.clone()))
        else {
            return;
        };
        let awake = carried_awake(self.interactive_stage().map(|previous| {
            let awake = previous.is_awake();
            previous.set_interactive(false);
            awake
        }));
        stage.set_interactive(true);
        stage.set_awake(awake);
        self.interactive.replace(Some(monitor));
        focus_prompt(&stage);
    }

    fn apply_look(&self, stage: &Stage) {
        let look = self.look.borrow();
        stage.stage.set_color(&look.color);
        let connector = stage.monitor.connector().unwrap_or_default();
        stage
            .stage
            .set_background(look.backgrounds.get(connector.as_str()));
        stage.stage.set_fit(look.fit);
        stage.stage.set_dim(look.dim);
        let clock = stage.stage.clock();
        let (time, date) = &look.clock;
        clock.set_formats(time, date);
        if let Some(now) = now() {
            clock.set_time(&now);
        }
        stage.stage.set_user(look.user.as_deref());
        stage.stage.set_avatar(look.avatar.as_ref());
        stage
            .stage
            .set_session_actions(ACTIONS.iter().filter_map(|action| {
                let key = action.key();
                look.session.get(key).map(|state| (key, state))
            }));
        stage.stage.track().set_track(look.track.as_ref());
        stage.stage.set_notifications(&look.notifications);
        stage.stage.set_battery(look.status.battery.as_ref());
        stage.stage.status().set_layout(look.status.layout.as_ref());
    }

    fn apply_prompt(&self, stage: &LockStage) {
        let state = self.prompt.borrow();
        let prompt = stage.prompt();
        prompt.set_available(state.available);
        stage.set_busy(state.busy);
        let kind = match state.lasting {
            true => MessageKind::Lasting,
            false => MessageKind::Error,
        };
        stage.set_message(state.message.as_deref(), kind);
    }

    fn each_prompt(&self, f: impl Fn(&PasswordPrompt)) {
        for stage in self.stages.borrow().iter() {
            f(stage.stage.prompt());
        }
    }

    fn tick(&self) {
        let Some(now) = now() else {
            return;
        };
        for stage in self.stages.borrow().iter() {
            stage.stage.clock().set_time(&now);
        }
    }

    fn paint_done(&self) {
        let left = self.unpainted.get().saturating_sub(1);
        self.unpainted.set(left);
        if left == 0 && !self.stages.borrow().is_empty() {
            (self.report)(Input::Painted(self.generation));
        }
    }
}

fn focus_prompt(stage: &LockStage) {
    if stage.session_open() {
        return;
    }
    match stage.is_awake() {
        true => stage.prompt().grab_focus(),
        false => stage.grab_focus(),
    };
}

fn build() -> (gtk4::Window, LockStage) {
    let stage = LockStage::new();
    let window = gtk4::Window::builder()
        .css_classes(["lock-surface"])
        .child(&stage)
        .build();
    (window, stage)
}

fn now() -> Option<glib::DateTime> {
    glib::DateTime::now_local().ok()
}

fn arm_clock(shared: &Rc<Shared>) {
    let delay = next_minute(now().map_or(f64::NAN, |now| now.seconds()));
    let weak = Rc::downgrade(shared);
    shared.clock.arm(delay, move || {
        let Some(shared) = weak.upgrade() else {
            return;
        };
        shared.clock.fired();
        shared.tick();
        arm_clock(&shared);
    });
}

fn on_first_paint(window: &gtk4::Window, shared: Weak<Shared>, painted: Rc<Cell<bool>>) {
    let Some(clock) = window.frame_clock() else {
        return;
    };
    let handler: Rc<RefCell<Option<SignalHandlerId>>> = Rc::default();
    let id = clock.connect_after_paint({
        let handler = handler.clone();
        move |clock| {
            if let Some(id) = handler.borrow_mut().take() {
                clock.disconnect(id);
            }
            if !painted.replace(true)
                && let Some(shared) = shared.upgrade()
            {
                shared.paint_done();
            }
        }
    });
    handler.replace(Some(id));
}

#[cfg(test)]
mod tests;
