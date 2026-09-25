use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::{Duration, Instant};

use adw::prelude::*;
use chrono::{DateTime, Local};
use gettextrs::gettext;
use gio_unix::DesktopAppInfo;
use glimpse_compositors::{Snapshot, detect_compositor};
use glimpse_config::ScreenshotConfig;
use glimpse_shot::doc::document::{Document, Mark, Source, Style, Tool};
use glimpse_shot::doc::esc::EscOutcome;
use glimpse_shot::doc::naming::file_name;
use glimpse_shot::doc::wire::{self, RectV1, ResultV1, TargetV1};
use glimpse_shot::ports::{Clipboard as _, Windows};
use glimpse_shot::session::{self, Effect, Event, Face, FinishKey, Outcome, Phase, Target};
use glimpse_wayland::capture::CaptureError;
use glimpse_wayland::selection::Entry;
use glimpse_widgets::{
    ShotBanner, ShotCountdown, ShotRegion, ShotToolbar, ShotWindowEntry, ShotWindowList,
    duration_ms, roundtrip,
};
use gtk4::{gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use super::canvas::{
    Canvas, Editor, palette, palette_index, resize_text_entry, set_cursor, spring_tool, stroke,
    text_entry, tool_name, tool_named,
};
use crate::adapters::clipboard::HoldClipboard;
use crate::adapters::clock::SystemClock;
use crate::adapters::files::Disk;
use crate::adapters::screens::WaylandScreens;
use crate::adapters::windows::{CompositorWindows, cropped, visible_crop};
use crate::cli::ScaleMode;
use crate::errors::OutputGone;
use crate::finish::{self, Destination};
use crate::headless::{self, Shot};
use crate::render::{self, Png};

const NAMESPACE: &str = "glimpse-shot";
const PROMPT_MAX: usize = 200;
const PORTAL_APP_MAX: usize = 200;
const FLASH_ALPHA: f64 = 0.6;
const TICK: Duration = Duration::from_millis(200);

#[derive(Clone)]
pub struct Options {
    pub target: Target,
    pub in_window: bool,
    pub delay: u32,
    pub cursor: bool,
    pub flash: bool,
    pub notify: bool,
    pub style: Style,
    pub prompt: Option<String>,
    pub portal: Option<String>,
    pub timeout: Option<u32>,
    pub json: bool,
    pub scale: ScaleMode,
    pub marks: Vec<Mark>,
    pub output: Option<PathBuf>,
    pub config: ScreenshotConfig,
    pub config_path: Option<PathBuf>,
    pub snapshot: Option<Snapshot>,
    pub tool: Option<Tool>,
}

pub enum Ended {
    Finished {
        outcome: Outcome,
        json: Option<String>,
    },
    HandedOff(Box<Handoff>),
    Failed(anyhow::Error),
}

pub struct Handoff {
    pub editor: Editor,
    pub target: Option<TargetV1>,
    pub taken: Option<DateTime<Local>>,
}

#[derive(Clone)]
struct Ui {
    window: gtk4::Window,
    canvas: Canvas,
    toolbar: ShotToolbar,
    revealer: gtk4::Revealer,
    list: ShotWindowList,
    region: ShotRegion,
    readout: gtk4::Label,
    text: gtk4::TextView,
    text_scroller: gtk4::ScrolledWindow,
    busy: adw::Spinner,
    toasts: adw::ToastOverlay,
}

struct Run {
    app: adw::Application,
    options: Options,
    clipboard: HoldClipboard,
    phase: RefCell<Phase>,
    target: RefCell<Option<TargetV1>>,
    taken: Cell<Option<DateTime<Local>>>,
    frozen: RefCell<Option<Source>>,
    png: RefCell<Option<Png>>,
    destination: RefCell<Option<Destination>>,
    error: RefCell<Option<anyhow::Error>>,
    ui: RefCell<Option<Ui>>,
    pill: RefCell<Option<gtk4::Window>>,
    watched: RefCell<Option<(gdk::Monitor, glib::SignalHandlerId)>>,
    animation: RefCell<Option<adw::TimedAnimation>>,
    flashed: Cell<bool>,
    enter_held: Cell<bool>,
    picking: Cell<Option<u64>>,
    launch_with: RefCell<Option<gio::AppInfo>>,
    over: Cell<bool>,
    ended: RefCell<Option<Finished>>,
}

type Finished = Box<dyn FnOnce(Ended)>;

pub struct Running(Rc<Run>);

impl Running {
    pub fn cancel(&self) {
        self.0.end(Outcome::Cancelled);
    }
}

pub fn start(
    app: &adw::Application,
    options: Options,
    ended: impl FnOnce(Ended) + 'static,
) -> Running {
    let face = Face {
        headless: options.portal.is_some(),
        notify: options.notify,
    };
    let until =
        (options.delay > 0).then(|| Instant::now() + Duration::from_secs(options.delay.into()));
    let (phase, effects) = session::start(options.target.clone(), until, face);
    let run = Rc::new(Run {
        app: app.clone(),
        clipboard: HoldClipboard {
            config: options.config_path.clone(),
        },
        options,
        phase: RefCell::new(phase),
        target: RefCell::default(),
        taken: Cell::default(),
        frozen: RefCell::default(),
        png: RefCell::default(),
        destination: RefCell::default(),
        error: RefCell::default(),
        ui: RefCell::default(),
        pill: RefCell::default(),
        watched: RefCell::default(),
        animation: RefCell::default(),
        flashed: Cell::new(false),
        enter_held: Cell::new(false),
        picking: Cell::default(),
        launch_with: RefCell::default(),
        over: Cell::new(false),
        ended: RefCell::new(Some(Box::new(ended))),
    });
    if let Some(seconds) = run.options.timeout {
        let weak = Rc::downgrade(&run);
        glib::timeout_add_seconds_local_once(seconds, move || {
            if let Some(run) = weak.upgrade() {
                run.send(Event::Timeout);
            }
        });
    }
    if let Some(until) = until {
        run.countdown(until);
    }
    run.effects(effects);
    Running(run)
}

impl Run {
    fn ui(&self) -> Option<Ui> {
        self.ui.borrow().clone()
    }

    fn send(self: &Rc<Self>, event: Event) {
        if self.over.get() {
            return;
        }
        let phase = self.phase.replace(Phase::Done(Outcome::Cancelled));
        let (phase, effects) = session::step(phase, event);
        self.phase.replace(phase);
        self.effects(effects);
    }

    fn effects(self: &Rc<Self>, effects: Vec<Effect>) {
        let events: Vec<Event> = effects
            .into_iter()
            .filter_map(|effect| self.perform(effect))
            .collect();
        for event in events {
            self.send(event);
        }
        let done = match &*self.phase.borrow() {
            Phase::Done(outcome) => Some(outcome.clone()),
            _ => None,
        };
        if let Some(outcome) = done {
            self.end(outcome);
        }
    }

    fn perform(self: &Rc<Self>, effect: Effect) -> Option<Event> {
        if self.over.get() {
            return None;
        }
        match effect {
            Effect::Capture(target) => Some(Event::Captured(self.capture(target))),
            Effect::Encode => {
                let encoded = self
                    .doc()
                    .ok_or_else(|| anyhow::anyhow!("nothing was captured"))
                    .and_then(|doc| {
                        render::export(&doc, headless::factor(self.options.scale, &doc))
                    });
                let result = encoded
                    .map(|png| drop(self.png.replace(Some(png))))
                    .map_err(|error| format!("{error:#}"));
                Some(Event::Effected(Effect::Encode, result))
            }
            Effect::Write(requested) => {
                let written = match (&*self.png.borrow(), &*self.destination.borrow()) {
                    (Some(png), Some(destination)) => finish::write(
                        destination,
                        &requested,
                        &Disk,
                        &png.bytes,
                        &mut std::io::stdout().lock(),
                    )
                    .map_err(|error| format!("{error:#}")),
                    _ => Err("nothing was encoded".to_owned()),
                };
                let launch = self.launch_with.take();
                Some(match written {
                    Ok(actual) => {
                        if let Some(app) = launch {
                            super::result::launch(&app, &actual);
                        }
                        Event::Effected(Effect::Write(actual), Ok(()))
                    }
                    Err(reason) => Event::Effected(Effect::Write(requested), Err(reason)),
                })
            }
            Effect::Copy => {
                let entry = self.png.borrow().as_ref().map(|png| Entry {
                    mime: "image/png".to_owned(),
                    data: Arc::from(png.bytes.as_slice()),
                });
                let copied = match entry {
                    Some(entry) => self.clipboard.copy(vec![entry]),
                    None => Err("nothing was encoded".to_owned()),
                };
                Some(Event::Effected(Effect::Copy, copied))
            }
            Effect::Notify(notice) => {
                if let Err(error) = self.clipboard.notify(notice) {
                    tracing::warn!("{error:#}");
                }
                None
            }
            Effect::ShowToast(text) => {
                self.toast(&toast_text(&text));
                None
            }
            Effect::OpenInWindow if self.options.json => {
                self.error.replace(Some(OutputGone.into()));
                self.end(Outcome::Failed(OutputGone.to_string()));
                None
            }
            Effect::OpenInWindow => {
                self.hand_off();
                None
            }
            Effect::Share(path) => Some(Event::Effected(Effect::Share(path), Ok(()))),
        }
    }

    fn doc(&self) -> Option<Document> {
        self.ui()?.canvas.read(|editor| editor.doc.clone())
    }

    fn toast(&self, text: &str) {
        if let Some(ui) = self.ui() {
            let toast = adw::Toast::new(text);
            toast.set_use_markup(false);
            ui.toasts.add_toast(toast);
        }
    }

    fn take(&self, target: Target) -> anyhow::Result<Shot> {
        self.taker().take(target)
    }

    fn taker(&self) -> Taker {
        Taker {
            snapshot: self.options.snapshot.clone(),
            cursor: self.options.cursor,
            scale: self.options.scale,
            marks: self.options.marks.clone(),
            clipboard: self.clipboard.clone(),
            frozen: self.frozen.borrow().clone(),
        }
    }

    fn capture(self: &Rc<Self>, target: Target) -> Result<Source, CaptureError> {
        let _hold = self.app.hold();
        if let Some(pill) = self.pill.take() {
            pill.destroy();
            barrier();
        }
        let frozen = match &target {
            Target::PickWindow | Target::PickRegion => Target::CurrentScreen,
            other => other.clone(),
        };
        match self.take(frozen) {
            Ok(shot) => {
                let source = shot.doc.source.clone();
                self.target.replace(Some(shot.target));
                self.taken.set(Some(shot.taken));
                if matches!(source, Source::Output { .. }) {
                    self.frozen.replace(Some(source.clone()));
                }
                match self.options.in_window
                    && !matches!(target, Target::PickWindow | Target::PickRegion)
                {
                    true => self.flash_then_hand_off(
                        Editor::new(shot.doc, self.options.style).starting_with(self.options.tool),
                    ),
                    false => self.open(shot.doc, &target),
                }
                Ok(source)
            }
            Err(error) => {
                let reason = format!("{error:#}");
                self.error.replace(Some(error));
                Err(CaptureError::Failed(reason))
            }
        }
    }

    fn countdown(self: &Rc<Self>, until: Instant) {
        let window = layer_window(KeyboardMode::None);
        window.set_anchor(Edge::Top, true);
        window.set_margin(Edge::Top, 18);
        let output = match &self.options.target {
            Target::Screen(name) => Some(name.clone()),
            _ => self
                .options
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.focused_output.clone()),
        };
        window.set_monitor(output.as_deref().and_then(monitor_named).as_ref());
        let pill = ShotCountdown::new();
        pill.set_seconds(remaining(until));
        let weak = Rc::downgrade(self);
        pill.connect_cancelled(move |_| {
            if let Some(run) = weak.upgrade() {
                run.send(Event::Esc);
            }
        });
        window.set_child(Some(&pill));
        window.present();
        self.pill.replace(Some(window));
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(TICK, move || {
            let Some(run) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !matches!(*run.phase.borrow(), Phase::Countdown { .. }) {
                return glib::ControlFlow::Break;
            }
            pill.set_seconds(remaining(until));
            run.send(Event::Tick(Instant::now()));
            glib::ControlFlow::Continue
        });
    }

    fn open(self: &Rc<Self>, doc: Document, target: &Target) {
        let connector = headless::output_of(&doc);
        let mut editor = Editor::new(doc, self.options.style).starting_with(self.options.tool);
        editor.list_open = *target == Target::PickWindow;
        editor.picking_region = *target == Target::PickRegion;
        let canvas = Canvas::new(editor);

        let toolbar = ShotToolbar::new();
        toolbar.set_can_focus(false);
        toolbar.set_color(palette_index(self.options.style.color));
        toolbar.set_stroke(self.options.style.stroke as u32);
        toolbar.set_target_visible(self.options.portal.is_none() && self.frozen.borrow().is_some());
        toolbar.set_finish_actions(finish_actions(self.options.json));
        toolbar.set_finish_visible(self.options.portal.is_none());
        let revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(duration_ms())
            .child(&toolbar)
            .build();

        let banner = ShotBanner::new();
        banner.set_can_focus(false);
        match (&self.options.portal, &self.options.prompt) {
            (Some(app), _) => {
                banner.set_portal_request(&glimpse_utils::text::clean(app, PORTAL_APP_MAX));
            }
            (None, Some(prompt)) => {
                banner.set_prompt(&glimpse_utils::text::clean(prompt, PROMPT_MAX));
            }
            (None, None) => banner.set_visible(false),
        }

        let list = ShotWindowList::new();
        list.set_windows(&self.entries());

        let column = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .spacing(8)
            .halign(gtk4::Align::Center)
            .valign(gtk4::Align::Start)
            .margin_top(14)
            .build();
        column.append(&revealer);
        column.append(&banner);
        column.append(&list);

        let region = ShotRegion::new();
        region.set_can_target(false);

        let readout = gtk4::Label::builder()
            .halign(gtk4::Align::End)
            .valign(gtk4::Align::End)
            .margin_end(16)
            .margin_bottom(16)
            .can_target(false)
            .build();
        readout.add_css_class("shot-readout");

        let (text_scroller, text) = overlay_text_entry();

        let busy = adw::Spinner::builder()
            .halign(gtk4::Align::Center)
            .valign(gtk4::Align::Center)
            .width_request(48)
            .height_request(48)
            .can_target(false)
            .visible(false)
            .build();

        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&canvas));
        overlay.add_overlay(&busy);
        overlay.add_overlay(&region);
        overlay.add_overlay(&readout);
        overlay.add_overlay(&text_scroller);
        overlay.add_overlay(&column);
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&overlay));

        let window = layer_window(KeyboardMode::Exclusive);
        window.set_exclusive_zone(-1);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        let monitor = monitor_named(&connector);
        window.set_monitor(monitor.as_ref());
        window.set_child(Some(&toasts));

        let ui = Ui {
            window,
            canvas,
            toolbar,
            revealer,
            list,
            region,
            readout,
            text,
            text_scroller,
            busy,
            toasts,
        };
        self.ui.replace(Some(ui.clone()));
        let weak = Rc::downgrade(self);
        ui.window.connect_destroy(move |_| {
            if let Some(run) = weak.upgrade()
                && !run.over.get()
            {
                run.ui.take();
                run.end(Outcome::Cancelled);
            }
        });
        self.wire(&ui);
        if let Some(monitor) = monitor {
            let weak = Rc::downgrade(self);
            let handler = monitor.connect_invalidate(move |_| {
                if let Some(run) = weak.upgrade() {
                    run.send(Event::OutputGone(connector.clone()));
                }
            });
            self.watched.replace(Some((monitor, handler)));
        }
        self.sync();
        ui.window.present();
    }

    fn wire(self: &Rc<Self>, ui: &Ui) {
        let weak = Rc::downgrade(self);
        ui.window.connect_map(move |_| {
            if let Some(run) = weak.upgrade() {
                run.flash();
            }
        });

        let drag = gtk4::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        let weak = Rc::downgrade(self);
        drag.connect_drag_begin(move |_, x, y| {
            with_run(&weak, |run, ui| {
                run.commit_text();
                ui.canvas.with(|editor| editor.press(x, y));
            })
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_update(move |gesture, dx, dy| {
            let shift = shifted(gesture);
            if let Some((x, y)) = gesture.start_point() {
                with_run(&weak, |_, ui| {
                    ui.canvas
                        .with(|editor| editor.motion(x + dx, y + dy, shift));
                })
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_end(move |gesture, dx, dy| {
            let shift = shifted(gesture);
            if let Some((x, y)) = gesture.start_point() {
                with_run(&weak, |run, ui| {
                    let picked = ui.canvas.with(|editor| {
                        let picking = editor.picking_region;
                        editor.release(x + dx, y + dy, shift);
                        picking && !editor.picking_region
                    });
                    if picked == Some(true)
                        && let Some(Source::Output { name, .. }) = &*run.frozen.borrow()
                    {
                        run.target
                            .replace(Some(TargetV1::Region { name: name.clone() }));
                    }
                    if picked == Some(true) && run.options.in_window {
                        run.hand_off_later_from_canvas();
                    }
                })
            }
        });
        ui.canvas.add_controller(drag);

        let hover = gtk4::EventControllerMotion::new();
        let weak = Rc::downgrade(self);
        hover.connect_motion(move |controller, x, y| {
            if controller
                .current_event_state()
                .intersects(gdk::ModifierType::BUTTON1_MASK | gdk::ModifierType::BUTTON3_MASK)
            {
                return;
            }
            if let Some(run) = weak.upgrade()
                && let Some(ui) = run.ui()
            {
                let name = ui.canvas.hover(x, y).unwrap_or("crosshair");
                set_cursor(&ui.canvas, name);
            }
        });
        hover.connect_leave(|controller| {
            if let Some(widget) = controller.widget() {
                set_cursor(&widget, "crosshair");
            }
        });
        ui.canvas.add_controller(hover);

        let resize = gtk4::GestureDrag::new();
        resize.set_button(gdk::BUTTON_SECONDARY);
        let weak = Rc::downgrade(self);
        resize.connect_drag_begin(move |gesture, x, y| {
            if shifted(gesture) {
                with_run(&weak, |run, ui| {
                    run.commit_text();
                    ui.canvas.with(|editor| editor.press_resize(x, y));
                    ui.canvas.grab_focus();
                });
            }
        });
        let weak = Rc::downgrade(self);
        resize.connect_drag_update(move |gesture, dx, dy| {
            if let Some((x, y)) = gesture.start_point() {
                with_run(&weak, |_, ui| {
                    ui.canvas.with(|editor| {
                        if editor.resizing_mark() {
                            editor.motion(x + dx, y + dy, false);
                        }
                    });
                });
            }
        });
        let weak = Rc::downgrade(self);
        resize.connect_drag_end(move |gesture, dx, dy| {
            if let Some((x, y)) = gesture.start_point() {
                with_run(&weak, |_, ui| {
                    ui.canvas.with(|editor| {
                        if editor.resizing_mark() {
                            editor.release(x + dx, y + dy, false);
                        }
                    });
                });
            }
        });
        ui.canvas.add_controller(resize.clone());

        let undo = gtk4::GestureClick::new();
        undo.set_button(gdk::BUTTON_SECONDARY);
        let weak = Rc::downgrade(self);
        undo.connect_pressed(move |gesture, _, _, _| {
            if gesture
                .current_event_state()
                .contains(gdk::ModifierType::SHIFT_MASK)
            {
                return;
            }
            with_run(&weak, |run, ui| {
                run.commit_text();
                run.edit(Editor::undo);
                ui.canvas.grab_focus();
            });
        });
        ui.canvas.add_controller(undo.clone());
        undo.group_with(&resize);

        let scroll = gtk4::EventControllerScroll::new(
            gtk4::EventControllerScrollFlags::VERTICAL | gtk4::EventControllerScrollFlags::DISCRETE,
        );
        let weak = Rc::downgrade(self);
        scroll.connect_scroll(move |controller, _, dy| {
            if controller.current_event_state().intersects(
                gdk::ModifierType::SHIFT_MASK
                    | gdk::ModifierType::CONTROL_MASK
                    | gdk::ModifierType::ALT_MASK,
            ) {
                return glib::Propagation::Proceed;
            }
            let Some(run) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let handled = run
                .ui()
                .and_then(|ui| ui.canvas.with(|editor| editor.scroll_stroke(dy)))
                .unwrap_or(false);
            if handled {
                run.sync();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        ui.canvas.add_controller(scroll);

        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, state| {
            let Some(run) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match key {
                gdk::Key::Return | gdk::Key::KP_Enter
                    if !state.contains(gdk::ModifierType::SHIFT_MASK) =>
                {
                    run.commit_text();
                    run.enter_held.set(true);
                    run.sync();
                }
                gdk::Key::Escape => run.escape(),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        ui.text.add_controller(keys);

        if self.options.portal.is_some() {
            let portal_keys = gtk4::EventControllerKey::new();
            portal_keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
            let weak = Rc::downgrade(self);
            portal_keys.connect_key_pressed(move |_, key, _, state| {
                let Some(run) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                run.portal_key_pressed(key, state)
            });
            ui.window.add_controller(portal_keys);
        }

        let weak = Rc::downgrade(self);
        ui.list.connect_window_picked(move |_, id| {
            if let Some(run) = weak.upgrade() {
                run.pick_window(id);
            }
        });

        let weak = Rc::downgrade(self);
        ui.toolbar.connect_tool_selected(move |_, name| {
            if let Some(tool) = tool_named(&name) {
                with_run(&weak, |run, ui| {
                    run.commit_text();
                    ui.canvas.with(|editor| editor.set_tool(tool));
                    ui.canvas.grab_focus();
                })
            }
        });
        let weak = Rc::downgrade(self);
        ui.toolbar.connect_target_selected(move |_, target| {
            if let Some(run) = weak.upgrade() {
                match target.as_str() {
                    "window" => run.open_list(),
                    _ => run.back_to_screen(),
                }
            }
        });
        let weak = Rc::downgrade(self);
        ui.toolbar.connect_color_selected(move |_, color| {
            with_run(&weak, |_, ui| {
                ui.canvas.with(|editor| editor.style.color = palette(color));
            })
        });
        let weak = Rc::downgrade(self);
        ui.toolbar.connect_stroke_selected(move |_, width| {
            with_run(&weak, |_, ui| {
                ui.canvas.with(|editor| editor.style.stroke = stroke(width));
            })
        });
        let weak = Rc::downgrade(self);
        ui.toolbar.connect_action(move |_, action| {
            let Some(run) = weak.upgrade() else {
                return;
            };
            run.commit_text();
            run.sync();
            match action.as_str() {
                "undo" => run.edit(Editor::undo),
                "redo" => run.edit(Editor::redo),
                "save" => run.finish(FinishKey::SaveAsClose),
                "save-as" => run.save_as(),
                "open-with" => run.open_with(),
                "open-window" => run.finish(FinishKey::OpenInWindow),
                "copy-close" => run.finish(FinishKey::CopyClose),
                "apply-crop" => run.edit(|editor| {
                    editor.apply_crop();
                }),
                _ => run.send(Event::Esc),
            }
            if let Some(ui) = run.ui() {
                ui.canvas.grab_focus();
            }
        });

        let shortcuts = gtk4::ShortcutController::new();
        let bind = |trigger: &str, letter: bool, act: fn(&Rc<Run>)| {
            let weak = Rc::downgrade(self);
            let enter = trigger.ends_with("Return") || trigger.ends_with("Enter");
            let action = gtk4::CallbackAction::new(move |_, _| {
                let Some(run) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if (letter && run.typing()) || (enter && run.listing()) {
                    return glib::Propagation::Proceed;
                }
                act(&run);
                glib::Propagation::Stop
            });
            shortcuts.add_shortcut(gtk4::Shortcut::new(
                gtk4::ShortcutTrigger::parse_string(trigger),
                Some(action),
            ));
        };
        bind("c", true, |run| run.pick_tool("crop"));
        bind("a", true, |run| run.pick_tool("arrow"));
        bind("l", true, |run| run.pick_tool("line"));
        bind("p", true, |run| run.pick_tool("freehand"));
        bind("h", true, |run| run.pick_tool("highlighter"));
        bind("f", true, |run| run.pick_tool("filled-rect"));
        bind("o", true, |run| run.pick_tool("oval"));
        bind("t", true, |run| run.pick_tool("text"));
        bind("s", true, Run::back_to_screen);
        bind("space", true, Run::whole_screen);
        if self.options.portal.is_none() {
            bind("w", true, Run::open_list);
        }
        bind("1", true, |run| run.pick_color(0));
        bind("2", true, |run| run.pick_color(1));
        bind("3", true, |run| run.pick_color(2));
        bind("4", true, |run| run.pick_color(3));
        bind("5", true, |run| run.pick_color(4));
        bind("bracketleft", true, |run| run.step_stroke(-1));
        bind("bracketright", true, |run| run.step_stroke(1));
        bind("<Control>z", true, |run| run.edit(Editor::undo));
        bind("<Control><Shift>z", true, |run| run.edit(Editor::redo));
        bind("Delete", true, |run| run.edit(Editor::delete));
        if self.options.portal.is_none() {
            bind("Escape", false, Run::escape);
            bind("Return", true, Run::enter);
            bind("KP_Enter", true, Run::enter);
            bind("<Shift>Return", true, |run| {
                run.finish(FinishKey::SaveAsClose)
            });
            bind("<Shift>KP_Enter", true, |run| {
                run.finish(FinishKey::SaveAsClose)
            });
            bind("<Control><Shift>s", true, Run::save_as);
            bind("<Control>c", true, |run| run.finish(FinishKey::Copy));
            bind("<Control>s", true, |run| run.finish(FinishKey::Save));
            bind("<Control>e", true, |run| {
                run.finish(FinishKey::OpenInWindow)
            });
        }
        ui.window.add_controller(shortcuts);

        let released = gtk4::EventControllerKey::new();
        released.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        released.connect_key_pressed(move |_, key, _, state| {
            let Some(tool) = spring_tool(key) else {
                return glib::Propagation::Proceed;
            };
            if state.intersects(
                gdk::ModifierType::CONTROL_MASK
                    | gdk::ModifierType::ALT_MASK
                    | gdk::ModifierType::SHIFT_MASK
                    | gdk::ModifierType::SUPER_MASK
                    | gdk::ModifierType::META_MASK,
            ) {
                return glib::Propagation::Proceed;
            }
            let Some(run) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if run.typing() || run.listing() {
                return glib::Propagation::Proceed;
            }
            if let Some(ui) = run.ui()
                && ui.canvas.with(|editor| editor.tool_key_down(tool)) == Some(true)
            {
                run.sync();
            }
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(self);
        released.connect_key_released(move |_, key, _, _| {
            if let Some(run) = weak.upgrade() {
                if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
                    run.enter_held.set(false);
                }
                if let Some(tool) = spring_tool(key)
                    && let Some(ui) = run.ui()
                    && ui.canvas.with(|editor| editor.tool_key_up(tool)) == Some(true)
                {
                    run.sync();
                }
            }
        });
        ui.window.add_controller(released);

        let weak = Rc::downgrade(self);
        ui.window.connect_is_active_notify(move |window| {
            if !window.is_active()
                && let Some(run) = weak.upgrade()
                && let Some(ui) = run.ui()
                && ui.canvas.with(Editor::cancel_tool_key).unwrap_or(false)
            {
                run.sync();
            }
        });
    }

    fn listing(&self) -> bool {
        self.ui()
            .and_then(|ui| ui.canvas.read(|editor| editor.list_open))
            .unwrap_or(false)
    }

    fn typing(&self) -> bool {
        self.ui()
            .and_then(|ui| ui.canvas.read(|editor| editor.typing.is_some()))
            .unwrap_or(false)
    }

    fn edit(self: &Rc<Self>, change: fn(&mut Editor)) {
        if let Some(ui) = self.ui() {
            ui.canvas.with(change);
        }
        self.sync();
    }

    fn enter(self: &Rc<Self>) {
        if self.enter_held.get() {
            return;
        }
        let applied = self
            .ui()
            .and_then(|ui| ui.canvas.with(Editor::apply_crop))
            .unwrap_or(false);
        match applied {
            true => {
                self.enter_held.set(true);
                self.sync();
            }
            false => self.finish(if self.options.portal.is_some() {
                FinishKey::Share
            } else {
                FinishKey::CopyClose
            }),
        }
    }

    fn portal_key_pressed(
        self: &Rc<Self>,
        key: gdk::Key,
        state: gdk::ModifierType,
    ) -> glib::Propagation {
        if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) && self.listing() {
            return glib::Propagation::Proceed;
        }
        let pending_crop = self
            .ui()
            .and_then(|ui| ui.canvas.read(|editor| editor.pending_crop.is_some()))
            .unwrap_or(false);
        match portal_key(key, state, pending_crop) {
            Some(PortalKey::Share | PortalKey::ApplyCrop) => {
                self.commit_text();
                self.enter();
            }
            Some(PortalKey::Refuse) => self.send(Event::Esc),
            None => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }

    fn pick_tool(self: &Rc<Self>, name: &str) {
        if let Some(tool) = tool_named(name) {
            self.edit_with(|editor| editor.set_tool(tool));
        }
    }

    fn pick_color(self: &Rc<Self>, index: u32) {
        self.edit_with(|editor| editor.style.color = palette(index));
    }

    fn step_stroke(self: &Rc<Self>, by: i32) {
        self.edit_with(|editor| editor.step_stroke(by));
    }

    fn edit_with(self: &Rc<Self>, change: impl FnOnce(&mut Editor)) {
        if let Some(ui) = self.ui() {
            ui.canvas.with(change);
        }
        self.sync();
    }

    fn commit_text(&self) {
        let Some(ui) = self.ui() else {
            return;
        };
        let buffer = ui.text.buffer();
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        ui.canvas.with(|editor| editor.commit_text(&text));
    }

    fn escape(self: &Rc<Self>) {
        let Some(ui) = self.ui() else {
            self.send(Event::Esc);
            return;
        };
        if self.picking.take().is_some() {
            ui.busy.set_visible(false);
            return;
        }
        match ui.canvas.with(Editor::escape) {
            Some(EscOutcome::Cancel) | None => self.send(Event::Esc),
            Some(_) => self.sync(),
        }
    }

    fn finish(self: &Rc<Self>, key: FinishKey) {
        if self.options.portal.is_some() && key != FinishKey::Share {
            return;
        }
        if key == FinishKey::OpenInWindow && self.options.json {
            return;
        }
        let destination =
            match Destination::of(self.options.output.as_deref(), false, &self.options.config) {
                Ok(destination) => destination,
                Err(error) => {
                    if self.options.portal.is_some() {
                        self.end(Outcome::Failed(format!("{error:#}")));
                    } else {
                        self.toast(&format!("{error:#}"));
                    }
                    return;
                }
            };
        let requested = self
            .taken
            .get()
            .and_then(|taken| destination.requested(&self.options.config, taken));
        let path = if key == FinishKey::Share {
            requested
        } else {
            finish_path(key, requested, self.options.json)
        };
        self.destination.replace(Some(destination));
        self.send(Event::Finish(key, path));
    }

    fn open_with(self: &Rc<Self>) {
        let Some(ui) = self.ui() else {
            return;
        };
        ui.window.set_visible(false);
        barrier();
        let weak = Rc::downgrade(self);
        super::result::choose_app(None, move |app| {
            let Some(run) = weak.upgrade() else {
                return;
            };
            match app {
                Some(app) => {
                    run.launch_with.replace(Some(app));
                    run.finish(FinishKey::SaveAsClose);
                }
                None => {
                    if let Some(ui) = run.ui() {
                        ui.window.present();
                        ui.canvas.grab_focus();
                    }
                }
            }
        });
    }

    fn save_as(self: &Rc<Self>) {
        let Some(ui) = self.ui() else {
            return;
        };
        ui.window.set_visible(false);
        barrier();
        let dialog = gtk4::FileDialog::builder()
            .title(gettext("Save Screenshot"))
            .modal(true)
            .build();
        let taken = self.taken.get().unwrap_or_else(Local::now);
        match Destination::of(self.options.output.as_deref(), false, &self.options.config) {
            Ok(Destination::File(path)) => {
                dialog.set_initial_file(Some(&gio::File::for_path(path)))
            }
            Ok(Destination::Directory(dir)) => {
                if dir.is_dir() {
                    dialog.set_initial_folder(Some(&gio::File::for_path(dir)));
                }
                dialog.set_initial_name(Some(&file_name(&self.options.config.filename, taken)));
            }
            _ => dialog.set_initial_name(Some(&file_name(&self.options.config.filename, taken))),
        }
        let weak = Rc::downgrade(self);
        dialog.save(
            None::<&gtk4::Window>,
            None::<&gio::Cancellable>,
            move |chosen| {
                let Some(run) = weak.upgrade() else {
                    return;
                };
                if let Some(path) = chosen.ok().and_then(|file| file.path()) {
                    run.destination
                        .replace(Some(Destination::File(path.clone())));
                    run.send(Event::Finish(FinishKey::SaveAsClose, Some(path)));
                }
                if let Some(ui) = run.ui() {
                    ui.window.present();
                    ui.canvas.grab_focus();
                }
            },
        );
    }

    fn whole_screen(self: &Rc<Self>) {
        let Some(ui) = self.ui() else {
            return;
        };
        if ui.canvas.read(|editor| editor.picking_region) != Some(true) {
            return;
        }
        ui.canvas.with(|editor| editor.picking_region = false);
        if let Some(Source::Output { name, .. }) = &*self.frozen.borrow() {
            self.target
                .replace(Some(TargetV1::Screen { name: name.clone() }));
        }
        self.sync();
        if self.options.in_window {
            self.hand_off_later_from_canvas();
        }
    }

    fn open_list(self: &Rc<Self>) {
        if self.options.portal.is_some() {
            return;
        }
        let Some(ui) = self.ui() else {
            return;
        };
        if self.frozen.borrow().is_some() && ui.canvas.read(|editor| !editor.drawn()) == Some(true)
        {
            ui.list.set_windows(&self.entries());
            ui.canvas.with(|editor| editor.list_open = true);
        }
        self.sync();
    }

    fn back_to_screen(self: &Rc<Self>) {
        let Some(ui) = self.ui() else {
            return;
        };
        let frozen = self.frozen.borrow().clone();
        let showing_window = ui
            .canvas
            .read(|editor| matches!(editor.doc.source, Source::Window { .. }));
        if let Some(frozen) = frozen
            && showing_window == Some(true)
            && ui.canvas.read(|editor| editor.drawn()) == Some(false)
        {
            if let Source::Output { name, .. } = &frozen {
                self.target
                    .replace(Some(TargetV1::Screen { name: name.clone() }));
            }
            ui.canvas.with(|editor| editor.replace_source(frozen));
        }
        ui.canvas.with(|editor| editor.list_open = false);
        self.sync();
    }

    fn pick_window(self: &Rc<Self>, id: u64) {
        if self.options.portal.is_some() {
            return;
        }
        let Some(ui) = self.ui() else {
            return;
        };
        if self.picking.get().is_some() {
            return;
        }
        self.picking.set(Some(id));
        ui.busy.set_visible(true);
        let taker = self.taker();
        let started = Instant::now();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let source = gio::spawn_blocking(move || taker.window_source(id)).await;
            let Some(run) = weak.upgrade() else {
                return;
            };
            if run.picking.get() != Some(id) || run.over.get() {
                return;
            }
            run.picking.set(None);
            tracing::debug!(elapsed = ?started.elapsed(), id, "the picked window arrived");
            match source {
                Ok(Ok(source)) => run.picked(source),
                Ok(Err(error)) => run.pick_failed(&format!("{error:#}")),
                Err(_) => run.pick_failed("the window capture stopped unexpectedly"),
            }
        });
    }

    fn picked(self: &Rc<Self>, source: Source) {
        let Some(ui) = self.ui() else {
            return;
        };
        ui.busy.set_visible(false);
        if let Source::Window {
            id, app_id, title, ..
        } = &source
        {
            self.target.replace(Some(TargetV1::Window {
                id: *id,
                app_id: app_id.clone(),
                title: title.clone(),
            }));
        }
        ui.canvas.with(|editor| {
            editor.replace_source(source);
            editor.list_open = false;
        });
        self.sync();
        if self.options.in_window {
            self.hand_off_later_from_canvas();
            return;
        }
        ui.canvas.grab_focus();
    }

    fn pick_failed(self: &Rc<Self>, reason: &str) {
        if let Some(ui) = self.ui() {
            ui.busy.set_visible(false);
        }
        self.toast(reason);
        self.sync();
    }

    fn entries(&self) -> Vec<ShotWindowEntry> {
        let (Some(snapshot), Some(Source::Output { name, .. })) =
            (&self.options.snapshot, &*self.frozen.borrow())
        else {
            return Vec::new();
        };
        let active = snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.is_active && workspace.output.as_deref() == Some(name))
            .map(|workspace| workspace.id);
        snapshot
            .windows
            .iter()
            .map(|window| {
                let info = window.app_id.as_deref().and_then(desktop_entry);
                let here = active.is_some() && window.workspace_id == active;
                ShotWindowEntry {
                    id: window.id.0,
                    app_id: window.app_id.clone(),
                    title: window.title.clone().unwrap_or_default(),
                    name: info.as_ref().map(|info| info.name().to_string()),
                    icon: info
                        .and_then(|info| info.icon())
                        .and_then(|icon| icon.downcast::<gio::ThemedIcon>().ok())
                        .and_then(|icon| icon.names().first().map(ToString::to_string)),
                    this_workspace: here,
                    focused: window.is_focused,
                    captured_when_picked: !(here
                        && window.is_floating
                        && window.output.as_deref() == Some(name)),
                }
            })
            .collect()
    }

    fn flash(self: &Rc<Self>) {
        let Some(ui) = self.ui() else {
            return;
        };
        if !self.listing() {
            ui.canvas.grab_focus();
        }
        if self.flashed.replace(true) {
            return;
        }
        if !(self.options.flash && animations_enabled()) {
            ui.revealer.set_reveal_child(true);
            return;
        }
        let canvas = ui.canvas.clone();
        let target = adw::CallbackAnimationTarget::new(move |alpha| {
            canvas.with(|editor| editor.flash = alpha);
        });
        let animation = flash_animation(&ui.canvas, &target);
        let revealer = ui.revealer.clone();
        animation.connect_done(move |_| revealer.set_reveal_child(true));
        animation.play();
        self.animation.replace(Some(animation));
    }

    fn sync(&self) {
        let Some(ui) = self.ui() else {
            return;
        };
        let Some(state) = ui.canvas.read(|editor| {
            let exported = editor.doc.exported_rect();
            let shown = match editor.doc.source {
                Source::Window { scale, .. } => {
                    Some((editor.view().factor * scale.0 * 100.0).round() as u32)
                }
                _ => None,
            };
            (
                editor.tool,
                editor.style,
                editor.history.can_undo(),
                editor.history.can_redo(),
                editor.typing.map(|at| editor.view().to_logical(at)),
                editor.list_open,
                editor.picking_region,
                editor.region(),
                matches!(editor.doc.source, Source::Window { .. }),
                readout(exported.width.0, exported.height.0, shown),
                editor.drawn(),
                editor.pending_crop.is_some(),
                editor.cursor(),
            )
        }) else {
            return;
        };
        let (
            tool,
            style,
            undo,
            redo,
            typing,
            list,
            picking,
            region,
            window,
            size,
            drawn,
            pending,
            cursor,
        ) = state;
        set_cursor(&ui.canvas, cursor);
        ui.toolbar.set_apply_visible(pending);
        ui.toolbar.set_tool(tool_name(tool));
        ui.toolbar.set_color(palette_index(style.color));
        ui.toolbar.set_stroke(style.stroke as u32);
        ui.toolbar.set_can_undo(undo);
        ui.toolbar.set_can_redo(redo);
        ui.toolbar
            .set_target(if list || window { "window" } else { "screen" });
        ui.toolbar.set_target_sensitive(!drawn);
        ui.list.set_visible(list);
        ui.revealer.set_visible(!picking);
        ui.region.set_visible(picking);
        ui.region
            .set_selection(region.map(|(selection, _)| selection));
        if let Some((_, crop)) = region {
            ui.region
                .set_size_text(&format!("{} × {}", crop.width.0, crop.height.0));
        }
        ui.readout.set_visible(!picking);
        ui.readout.set_text(&size);
        match typing {
            Some((x, y)) if !ui.text_scroller.is_visible() => {
                ui.text_scroller.set_margin_start(x.round() as i32);
                ui.text_scroller.set_margin_top(y.round() as i32);
                ui.text_scroller
                    .set_max_content_height((ui.canvas.height() - y.round() as i32 - 16).max(40));
                ui.text.buffer().set_text("");
                ui.text_scroller.set_visible(true);
                ui.text.grab_focus();
            }
            None if ui.text_scroller.is_visible() => {
                ui.text_scroller.set_visible(false);
                ui.canvas.grab_focus();
            }
            _ => {}
        }
    }

    fn hand_off(self: &Rc<Self>) {
        let Some(editor) = self.ui().and_then(|ui| ui.canvas.take_editor()) else {
            return;
        };
        self.give(editor);
    }

    fn hand_off_later_from_canvas(self: &Rc<Self>) {
        if let Some(editor) = self.ui().and_then(|ui| ui.canvas.take_editor()) {
            self.hand_off_later(editor);
        }
    }

    fn flash_then_hand_off(self: &Rc<Self>, editor: Editor) {
        if !flashes(self.options.flash, animations_enabled(), &editor.doc.source) {
            return self.hand_off_later(editor);
        }
        let window = layer_window(KeyboardMode::None);
        window.add_css_class("shot-flash");
        window.set_exclusive_zone(-1);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_monitor(monitor_named(&headless::output_of(&editor.doc)).as_ref());
        let alpha = Rc::new(Cell::new(FLASH_ALPHA));
        let area = gtk4::DrawingArea::new();
        area.set_can_target(false);
        let shade = Rc::clone(&alpha);
        area.set_draw_func(move |_, cairo, _, _| {
            cairo.set_source_rgba(1.0, 1.0, 1.0, shade.get());
            if let Err(error) = cairo.paint() {
                tracing::warn!("cannot paint the flash: {error}");
            }
        });
        window.set_child(Some(&area));
        let slot = Rc::new(RefCell::new(Some(editor)));
        let weak = Rc::downgrade(self);
        window.connect_map(move |window| play_flash(window, &weak, &slot, &alpha));
        self.pill.replace(Some(window.clone()));
        window.present();
    }

    fn hand_off_later(self: &Rc<Self>, editor: Editor) {
        let weak = Rc::downgrade(self);
        let slot = RefCell::new(Some(editor));
        glib::idle_add_local_once(move || {
            if let (Some(run), Some(editor)) = (weak.upgrade(), slot.take()) {
                run.give(editor);
            }
        });
    }

    fn give(self: &Rc<Self>, mut editor: Editor) {
        if self.over.replace(true) {
            return;
        }
        editor.list_open = false;
        editor.picking_region = false;
        let handoff = Handoff {
            editor,
            target: self.target.take(),
            taken: self.taken.get(),
        };
        self.close();
        if let Some(ended) = self.ended.take() {
            ended(Ended::HandedOff(Box::new(handoff)));
        }
    }

    fn end(&self, outcome: Outcome) {
        if self.over.replace(true) {
            return;
        }
        let ended = match (outcome, self.error.take()) {
            (Outcome::Failed(_), Some(error)) => Ended::Failed(error),
            (outcome, _) => Ended::Finished {
                json: self.json_line(&outcome),
                outcome,
            },
        };
        self.close();
        if let Some(callback) = self.ended.take() {
            callback(ended);
        }
    }

    fn close(&self) {
        if let Some(pill) = self.pill.take() {
            pill.destroy();
        }
        if let Some((monitor, handler)) = self.watched.take() {
            monitor.disconnect(handler);
        }
        if let Some(ui) = self.ui.take() {
            ui.window.destroy();
        }
    }

    fn json_line(&self, outcome: &Outcome) -> Option<String> {
        if !self.options.json {
            return None;
        }
        let doc = self.doc()?;
        let png = self.png.borrow();
        result_line(outcome, &doc, png.as_ref()?, self.target.take()?)
    }
}

struct Taker {
    snapshot: Option<Snapshot>,
    cursor: bool,
    scale: ScaleMode,
    marks: Vec<Mark>,
    clipboard: HoldClipboard,
    frozen: Option<Source>,
}

impl Taker {
    fn take(&self, target: Target) -> anyhow::Result<Shot> {
        let screens = WaylandScreens {
            compositor: self.snapshot.is_some(),
            focused: self
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.focused_output.clone()),
            counted: RefCell::default(),
        };
        let windows = self.snapshot.clone().map(|snapshot| CompositorWindows {
            compositor: detect_compositor(),
            snapshot,
            clipboard: &self.clipboard,
            restore: true,
        });
        let deps = headless::Deps {
            screens: &screens,
            windows: windows.as_ref().map(|windows| windows as &dyn Windows),
            clipboard: &self.clipboard,
            files: &Disk,
            clock: &SystemClock,
            outputs: self
                .snapshot
                .as_ref()
                .map(headless::placed)
                .unwrap_or_default(),
            encode: &render::export,
            step: session::step,
        };
        headless::take(target, self.cursor, self.scale, &self.marks, &deps)
    }

    fn window_source(&self, id: u64) -> anyhow::Result<Source> {
        let window = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.windows.iter().find(|window| window.id.0 == id));
        if let (Some(window), Some(snapshot), Some(Source::Output { name, scale, frame })) =
            (window, self.snapshot.as_ref(), &self.frozen)
            && window.is_floating
            && window.output.as_deref() == Some(name.as_str())
            && let Some(crop) = visible_crop(window, snapshot)
        {
            return Ok(Source::Window {
                id,
                app_id: window.app_id.clone(),
                title: window.title.clone(),
                scale: *scale,
                frame: cropped(frame, crop),
            });
        }
        Ok(self.take(Target::Window(id))?.doc.source)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PortalKey {
    Share,
    ApplyCrop,
    Refuse,
}

fn portal_key(key: gdk::Key, state: gdk::ModifierType, pending_crop: bool) -> Option<PortalKey> {
    let modified = state.intersects(
        gdk::ModifierType::SHIFT_MASK
            | gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::ALT_MASK
            | gdk::ModifierType::SUPER_MASK
            | gdk::ModifierType::HYPER_MASK
            | gdk::ModifierType::META_MASK,
    );
    if modified {
        return None;
    }
    match key {
        gdk::Key::Return | gdk::Key::KP_Enter if pending_crop => Some(PortalKey::ApplyCrop),
        gdk::Key::Return | gdk::Key::KP_Enter => Some(PortalKey::Share),
        gdk::Key::Escape => Some(PortalKey::Refuse),
        _ => None,
    }
}

fn finish_actions(json: bool) -> &'static [&'static str] {
    match json {
        true => &["save", "save-as", "open-with"],
        false => &["save", "save-as", "open-with", "open-window"],
    }
}

fn finish_path(key: FinishKey, requested: Option<PathBuf>, json: bool) -> Option<PathBuf> {
    match key {
        FinishKey::CopyClose | FinishKey::Copy => requested.filter(|_| json),
        FinishKey::Save | FinishKey::SaveAsClose => requested,
        FinishKey::OpenInWindow | FinishKey::Share => None,
    }
}

pub fn result_line(
    outcome: &Outcome,
    doc: &Document,
    png: &Png,
    target: TargetV1,
) -> Option<String> {
    let (path, copied) = match outcome {
        Outcome::Saved { path, copied } => (Some(path.clone()), *copied),
        Outcome::Shared(path) => (Some(path.clone()), false),
        Outcome::Copied => (None, true),
        _ => return None,
    };
    let (annotations, dropped) = wire::annotations(doc);
    let result = ResultV1 {
        version: 1,
        path: path
            .filter(|path| path != Path::new(finish::STDOUT))
            .map(|path| std::path::absolute(&path).unwrap_or(path)),
        copied,
        width: png.width,
        height: png.height,
        scale: headless::scale_of(doc),
        output: Some(headless::output_of(doc)),
        target,
        crop: doc.crop.map(RectV1::from),
        dropped,
        annotations,
    };
    serde_json::to_string(&result).ok()
}

fn readout(width: i32, height: i32, shown: Option<u32>) -> String {
    let size = format!("{width} × {height}");
    match shown {
        Some(percent) if percent < 100 => gettext("{size} · scaled {percent}%")
            .replace("{size}", &size)
            .replace("{percent}", &percent.to_string()),
        _ => size,
    }
}

fn with_run(weak: &Weak<Run>, act: impl FnOnce(&Rc<Run>, &Ui)) {
    let Some(run) = weak.upgrade() else {
        return;
    };
    if let Some(ui) = run.ui() {
        act(&run, &ui);
    }
    run.sync();
}

fn shifted(gesture: &gtk4::GestureDrag) -> bool {
    gesture
        .current_event_state()
        .contains(gdk::ModifierType::SHIFT_MASK)
}

fn flashes(flash: bool, animated: bool, source: &Source) -> bool {
    flash && animated && matches!(source, Source::Output { .. })
}

fn animations_enabled() -> bool {
    duration_ms() > 0
        && gtk4::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations())
}

fn play_flash(
    window: &gtk4::Window,
    run: &std::rc::Weak<Run>,
    slot: &Rc<RefCell<Option<Editor>>>,
    alpha: &Rc<Cell<f64>>,
) {
    let painted = window.child();
    let shade = Rc::clone(alpha);
    let target = adw::CallbackAnimationTarget::new(move |value| {
        shade.set(value);
        if let Some(area) = &painted {
            area.queue_draw();
        }
    });
    let animation = flash_animation(window, &target);
    let (done, slot) = (run.clone(), Rc::clone(slot));
    animation.connect_done(move |_| {
        let (Some(run), Some(editor)) = (done.upgrade(), slot.take()) else {
            return;
        };
        if let Some(flash) = run.pill.take() {
            flash.destroy();
        }
        run.give(editor);
    });
    animation.play();
    if let Some(run) = run.upgrade() {
        run.animation.replace(Some(animation));
    }
}

fn flash_animation(
    widget: &impl IsA<gtk4::Widget>,
    target: &impl IsA<adw::AnimationTarget>,
) -> adw::TimedAnimation {
    adw::TimedAnimation::builder()
        .widget(widget)
        .value_from(FLASH_ALPHA)
        .value_to(0.0)
        .duration(duration_ms())
        .easing(adw::Easing::EaseInCubic)
        .target(target)
        .build()
}

fn layer_window(keyboard: KeyboardMode) -> gtk4::Window {
    let window = gtk4::Window::new();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(keyboard);
    window
}

fn monitor_named(connector: &str) -> Option<gdk::Monitor> {
    let display = gdk::Display::default()?;
    display
        .monitors()
        .iter::<gdk::Monitor>()
        .flatten()
        .find(|monitor| monitor.connector().is_some_and(|name| name == connector))
}

fn barrier() {
    if let Some(display) = gdk::Display::default()
        && let Err(error) = roundtrip(&display)
    {
        tracing::warn!("no unmap barrier: {error}");
    }
}

fn remaining(until: Instant) -> u32 {
    until
        .saturating_duration_since(Instant::now())
        .as_secs_f64()
        .ceil() as u32
}

fn desktop_entry(app_id: &str) -> Option<DesktopAppInfo> {
    if app_id.is_empty() || app_id.contains('/') || app_id.contains('\0') {
        return None;
    }
    DesktopAppInfo::new(&format!("{app_id}.desktop"))
}

fn overlay_text_entry() -> (gtk4::ScrolledWindow, gtk4::TextView) {
    let (scroller, text) = text_entry();
    scroller.set_halign(gtk4::Align::Start);
    scroller.set_valign(gtk4::Align::Start);
    scroller.set_visible(false);
    scroller.add_css_class("shot-surface");
    text.buffer().connect_changed(glib::clone!(
        #[weak]
        text,
        #[weak]
        scroller,
        move |_| {
            let text = text.clone();
            let scroller = scroller.clone();
            glib::idle_add_local_once(move || {
                let max_height = scroller
                    .parent()
                    .map(|parent| parent.height() - scroller.margin_top() - 16)
                    .unwrap_or(600)
                    .max(40);
                scroller.set_max_content_height(max_height);
                resize_text_entry(&text, &scroller, max_height);
            });
        }
    ));
    (scroller, text)
}

fn toast_text(text: &str) -> String {
    match text {
        "Saved" => gettext("Saved"),
        "Copied" => gettext("Copied"),
        other => other.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> glimpse_wayland::capture::Frame {
        glimpse_wayland::capture::Frame {
            connector: "eDP-1".to_owned(),
            width: 2,
            height: 2,
            pixels: glib::Bytes::from_owned(vec![0u8; 16]),
            alpha: false,
        }
    }

    #[test]
    fn only_a_screen_capture_flashes_and_only_when_asked_and_animated() {
        let screen = Source::Output {
            name: "eDP-1".to_owned(),
            scale: glimpse_shot::doc::units::Scale(1.0),
            frame: frame(),
        };
        let window = Source::Window {
            id: 7,
            app_id: None,
            title: None,
            scale: glimpse_shot::doc::units::Scale(1.0),
            frame: frame(),
        };
        assert!(flashes(true, true, &screen));
        assert!(!flashes(false, true, &screen));
        assert!(!flashes(true, false, &screen));
        assert!(!flashes(true, true, &window));
    }

    #[test]
    fn shift_enter_saves_to_the_default_path_and_copy_names_it_only_for_json() {
        let default = Some(PathBuf::from("/home/me/Pictures/Screenshots/shot.png"));
        assert_eq!(
            finish_path(FinishKey::SaveAsClose, default.clone(), false),
            default,
            "Shift+Enter needs no dialog"
        );
        assert_eq!(
            finish_path(FinishKey::Save, default.clone(), false),
            default
        );
        assert_eq!(
            finish_path(FinishKey::CopyClose, default.clone(), false),
            None
        );
        assert_eq!(
            finish_path(FinishKey::CopyClose, default.clone(), true),
            default
        );
        assert_eq!(finish_path(FinishKey::OpenInWindow, default, false), None);
        let (state, effects) = session::step(
            Phase::Editing {
                saved: None,
                copied: false,
                face: Face {
                    headless: false,
                    notify: false,
                },
            },
            Event::Finish(
                FinishKey::SaveAsClose,
                Some(PathBuf::from("/home/me/Pictures/Screenshots/shot.png")),
            ),
        );
        assert_eq!(effects, [Effect::Encode], "{state:?}");
    }

    #[test]
    fn the_finish_menu_hides_open_in_window_under_json() {
        assert_eq!(
            finish_actions(false),
            ["save", "save-as", "open-with", "open-window"]
        );
        assert_eq!(finish_actions(true), ["save", "save-as", "open-with"]);
    }

    #[test]
    fn portal_keys_only_share_apply_crop_or_refuse() {
        let none = gdk::ModifierType::empty();
        assert_eq!(
            portal_key(gdk::Key::Return, none, false),
            Some(PortalKey::Share)
        );
        assert_eq!(
            portal_key(gdk::Key::KP_Enter, none, true),
            Some(PortalKey::ApplyCrop)
        );
        assert_eq!(
            portal_key(gdk::Key::Escape, none, false),
            Some(PortalKey::Refuse)
        );
        for (key, state) in [
            (gdk::Key::Return, gdk::ModifierType::SHIFT_MASK),
            (gdk::Key::c, gdk::ModifierType::CONTROL_MASK),
            (gdk::Key::s, gdk::ModifierType::CONTROL_MASK),
            (
                gdk::Key::s,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK,
            ),
            (gdk::Key::e, gdk::ModifierType::CONTROL_MASK),
        ] {
            assert_eq!(portal_key(key, state, false), None);
        }
    }

    #[test]
    fn portal_share_result_names_the_absolute_file_written_from_a_relative_directory() {
        let folder = tempfile::tempdir_in(".").expect("temporary screenshot folder");
        let relative = PathBuf::from(".").join(folder.path().file_name().expect("folder name"));
        let config = ScreenshotConfig {
            directory: Some(relative),
            ..ScreenshotConfig::default()
        };
        let destination = Destination::of(None, false, &config).expect("save destination");
        let requested = destination
            .requested(&config, Local::now())
            .expect("requested screenshot path");
        let png = Png {
            bytes: include_bytes!("../../tests/golden/every_mark.png").to_vec(),
            width: 2,
            height: 2,
        };
        let actual = finish::write(&destination, &requested, &Disk, &png.bytes, &mut Vec::new())
            .expect("screenshot written");
        let document = Document {
            source: Source::Output {
                name: "eDP-1".to_owned(),
                scale: glimpse_shot::doc::units::Scale(1.0),
                frame: frame(),
            },
            crop: None,
            marks: Vec::new(),
            selected: None,
        };
        let line = result_line(
            &Outcome::Shared(actual.clone()),
            &document,
            &png,
            TargetV1::Screen {
                name: "eDP-1".to_owned(),
            },
        )
        .expect("result object");
        let result: ResultV1 = serde_json::from_str(&line).expect("result json");
        let absolute = std::path::absolute(&actual).expect("absolute result path");
        assert_eq!(result.path.as_deref(), Some(absolute.as_path()));
        assert!(result.path.as_ref().is_some_and(|path| path.is_absolute()));
        assert!(!result.copied);
        assert_eq!(std::fs::read(actual).expect("saved image"), png.bytes);
    }

    #[test]
    #[ignore = "needs a display"]
    fn a_cancelled_run_answers_once_as_cancelled_and_stays_off_the_app() {
        if gtk4::init().is_err() || adw::init().is_err() {
            return;
        }
        assert!(glimpse_widgets::register_resources().is_ok());
        let app = adw::Application::builder()
            .application_id("me.aresa.Glimpse.Shot.Test")
            .build();
        let ended = Rc::new(RefCell::new(Vec::new()));
        let options = Options {
            target: Target::Screen("eDP-1".to_owned()),
            in_window: false,
            delay: 60,
            cursor: false,
            flash: false,
            notify: false,
            style: Style {
                color: glimpse_config::PaletteColor::Red,
                stroke: glimpse_config::Stroke::Medium,
            },
            prompt: None,
            portal: None,
            timeout: None,
            json: false,
            scale: ScaleMode::Buffer,
            marks: Vec::new(),
            output: None,
            config: glimpse_config::ScreenshotConfig::default(),
            config_path: None,
            snapshot: None,
            tool: None,
        };
        let running = start(&app, options, {
            let ended = Rc::clone(&ended);
            move |end| ended.borrow_mut().push(end)
        });
        assert!(running.0.pill.borrow().is_some(), "the countdown is up");
        assert!(
            app.windows().is_empty(),
            "no overlay window belongs to the app"
        );
        running.cancel();
        running.cancel();
        let ended = ended.borrow();
        assert_eq!(ended.len(), 1);
        assert!(matches!(
            ended[0],
            Ended::Finished {
                outcome: Outcome::Cancelled,
                ..
            }
        ));
        assert!(running.0.pill.borrow().is_none(), "the countdown is gone");
    }

    #[test]
    fn a_window_shown_below_its_logical_size_says_how_much() {
        assert_eq!(readout(2551, 2055, Some(62)), "2551 × 2055 · scaled 62%");
        assert_eq!(readout(800, 600, Some(100)), "800 × 600");
        assert_eq!(readout(1440, 612, None), "1440 × 612");
    }

    #[test]
    fn a_countdown_rounds_up_to_the_next_whole_second() {
        assert_eq!(remaining(Instant::now() + Duration::from_millis(2100)), 3);
        assert_eq!(remaining(Instant::now()), 0);
    }

    #[test]
    #[ignore = "needs a display"]
    fn overlay_text_entry_is_readable_and_grows() {
        gtk4::init().expect("nested display");
        adw::init().expect("libadwaita");
        let _styles = glimpse_widgets::Styles::install(adw::ColorScheme::Default);
        let canvas = gtk4::DrawingArea::new();
        canvas.set_size_request(800, 600);
        let (scroller, text) = overlay_text_entry();
        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&canvas));
        overlay.add_overlay(&scroller);
        let window = gtk4::Window::new();
        window.set_child(Some(&overlay));
        window.present();
        let settle = || {
            let context = glib::MainContext::default();
            for _ in 0..30 {
                while context.pending() {
                    context.iteration(false);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        settle();
        scroller.set_visible(true);
        settle();
        let initial = (text.width(), scroller.height());
        text.buffer().set_text("first\nsecond\nthird\nfourth");
        settle();
        for _ in 0..270 {
            if scroller.height() >= scroller.height_request() {
                break;
            }
            while glib::MainContext::default().pending() {
                glib::MainContext::default().iteration(false);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let grown = (text.width(), scroller.height());
        assert!(
            initial.0 > 100 && initial.1 > 20,
            "text entry is tiny: {initial:?}"
        );
        assert!(
            grown.1 > initial.1 + 10,
            "text entry did not grow: {initial:?} -> {grown:?}, request={}, end={:?}, lines={}, mapped={}/{}/{}, window={}x{}",
            scroller.height_request(),
            text.iter_location(&text.buffer().end_iter()),
            text.buffer().line_count(),
            scroller.is_mapped(),
            overlay.is_mapped(),
            window.is_mapped(),
            window.width(),
            window.height(),
        );
        text.buffer().set_text(&"line\n".repeat(200));
        settle();
        assert!(scroller.height() <= canvas.height());
        let adjustment = scroller.vadjustment();
        assert!(
            adjustment.upper() > adjustment.page_size(),
            "height={} canvas={} adjustment={}/{} end={:?}",
            scroller.height(),
            canvas.height(),
            adjustment.upper(),
            adjustment.page_size(),
            text.iter_location(&text.buffer().end_iter()),
        );
    }
}
