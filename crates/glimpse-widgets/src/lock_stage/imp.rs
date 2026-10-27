use gtk4::{CompositeTemplate, TemplateChild, gdk, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use crate::{Indicator, LockClock, PasswordPrompt, SessionSheet, StatusIsland, TrackCard};

pub const AWAKE: &str = "lock-stage--awake";
const COVER_AFTER: Duration = Duration::from_secs(10);
const IDLE_CAP_AFTER: Duration = Duration::from_secs(60);
const WAKE_DISTANCE: f64 = 12.0;
const INPUT_SETTLE: Duration = Duration::from_millis(300);
const CHORD: gdk::ModifierType = gdk::ModifierType::CONTROL_MASK
    .union(gdk::ModifierType::ALT_MASK)
    .union(gdk::ModifierType::SUPER_MASK)
    .union(gdk::ModifierType::META_MASK);
const BLUR_WIDTH: i32 = 480;
const BLUR_RADIUS: f32 = 10.0;

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/lock_stage.ui")]
pub struct LockStage {
    #[template_child]
    pub overlay: TemplateChild<gtk4::Overlay>,
    #[template_child]
    pub base: TemplateChild<gtk4::Picture>,
    #[template_child]
    pub background: TemplateChild<gtk4::Picture>,
    #[template_child]
    pub blurred: TemplateChild<gtk4::Picture>,
    #[template_child]
    pub scrim: TemplateChild<gtk4::Box>,
    #[template_child]
    pub layout: TemplateChild<gtk4::CenterBox>,
    #[template_child]
    pub corner: TemplateChild<gtk4::Overlay>,
    #[template_child]
    pub status: TemplateChild<StatusIsland>,
    #[template_child]
    pub clock: TemplateChild<LockClock>,
    #[template_child]
    pub reveal: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub initials: TemplateChild<gtk4::Label>,
    #[template_child]
    pub photo: TemplateChild<gtk4::Picture>,
    #[template_child]
    pub prompt: TemplateChild<PasswordPrompt>,
    #[template_child]
    pub bottom: TemplateChild<adw::Bin>,
    #[template_child]
    pub track: TemplateChild<TrackCard>,
    #[template_child]
    pub cover: TemplateChild<gtk4::Box>,
    #[template_child]
    pub cover_notifications: TemplateChild<Indicator>,
    #[template_child]
    pub cover_battery: TemplateChild<Indicator>,
    #[template_child]
    pub cover_power: TemplateChild<gtk4::Image>,
    #[template_child]
    pub sheet: TemplateChild<SessionSheet>,

    pub dim: Cell<f64>,
    pub color: Cell<Option<gdk::RGBA>>,
    pub awake: Cell<bool>,
    pub interactive: Cell<bool>,
    pub origin: Cell<Option<(f64, f64)>>,
    pub idle: RefCell<Option<glib::SourceId>>,
    pub active: Cell<Option<Instant>>,
    pub hidden: Cell<Option<[bool; 4]>>,
    pub woke_by_key: Cell<Option<Instant>>,
    pub message: RefCell<(Option<String>, crate::MessageKind)>,
    pub user: RefCell<Option<String>>,
    pub groups: RefCell<Vec<crate::ChipGroup>>,
    #[cfg(test)]
    pub timing: Cell<Timing>,
}

#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub soft: Duration,
    pub cap: Duration,
}

const TIMING: Timing = Timing {
    soft: COVER_AFTER,
    cap: IDLE_CAP_AFTER,
};

#[cfg(test)]
impl Default for Timing {
    fn default() -> Self {
        TIMING
    }
}

fn modifier(key: gdk::Key) -> bool {
    matches!(
        key,
        gdk::Key::Shift_L
            | gdk::Key::Shift_R
            | gdk::Key::Control_L
            | gdk::Key::Control_R
            | gdk::Key::Alt_L
            | gdk::Key::Alt_R
            | gdk::Key::Super_L
            | gdk::Key::Super_R
            | gdk::Key::Meta_L
            | gdk::Key::Meta_R
            | gdk::Key::ISO_Level3_Shift
            | gdk::Key::Caps_Lock
            | gdk::Key::Num_Lock
            | gdk::Key::Shift_Lock
    )
}

impl LockStage {
    fn key_pressed(
        &self,
        keys: &gtk4::EventControllerKey,
        key: gdk::Key,
        state: gdk::ModifierType,
    ) -> glib::Propagation {
        if !self.awake.get() {
            if modifier(key) || key == gdk::Key::Escape || !self.interactive.get() {
                return glib::Propagation::Proceed;
            }
            self.set_awake(true);
            self.woke_by_key.set(Some(Instant::now()));
            self.type_waking(keys, key, state);
            return glib::Propagation::Stop;
        }
        self.touch();
        if key != gdk::Key::Escape {
            let settling = self
                .woke_by_key
                .get()
                .is_some_and(|at| !self.prompt.is_mapped() || at.elapsed() < INPUT_SETTLE);
            if settling && !modifier(key) && !state.intersects(CHORD) {
                self.type_waking(keys, key, state);
                return glib::Propagation::Stop;
            }
            return glib::Propagation::Proceed;
        }
        if self.sheet.is_open() {
            self.sheet.close();
            return glib::Propagation::Stop;
        }
        if self.prompt.is_empty() && !self.prompt.is_busy() {
            self.set_awake(false);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    }

    fn type_waking(
        &self,
        keys: &gtk4::EventControllerKey,
        key: gdk::Key,
        state: gdk::ModifierType,
    ) {
        if state.intersects(CHORD) {
            return;
        }
        if matches!(
            key,
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter
        ) {
            self.prompt.submit();
        } else if !self.prompt.forward_key(keys)
            && let Some(typed) = key.to_unicode().filter(|typed| !typed.is_control())
        {
            self.prompt.preempt(typed);
        }
    }

    pub(super) fn set_awake(&self, awake: bool) {
        if awake && !self.interactive.get() {
            return;
        }
        if self.awake.replace(awake) == awake {
            return;
        }
        let stage = self.obj();
        crate::set_css_class(&*stage, AWAKE, awake);
        self.reveal.set_reveal_child(awake);
        self.status.set_can_target(awake);
        self.bottom.set_can_target(awake);
        self.sync_accessible();
        self.origin.set(None);
        if awake {
            self.prompt.grab_focus();
            self.touch();
        } else {
            self.sheet.close();
            self.prompt.clear();
            self.settle_message();
            self.active.set(None);
            self.woke_by_key.set(None);
            if let Some(source) = self.idle.take() {
                source.remove();
            }
            if self.interactive.get() {
                stage.grab_focus();
            }
        }
    }

    fn settle_message(&self) {
        let lasting = self.message.borrow().1 == crate::MessageKind::Lasting;
        if !lasting && self.prompt.is_available() {
            self.prompt.set_message(None, crate::MessageKind::Info);
        }
    }

    fn timing(&self) -> Timing {
        #[cfg(test)]
        {
            self.timing.get()
        }
        #[cfg(not(test))]
        {
            TIMING
        }
    }

    fn sync_accessible(&self) {
        let awake = self.awake.get();
        let hidden = [!awake, !awake, !awake, awake];
        if self.hidden.replace(Some(hidden)) == Some(hidden) {
            return;
        }
        for (widget, hidden) in [
            (self.status.upcast_ref::<gtk4::Widget>(), hidden[0]),
            (self.bottom.upcast_ref(), hidden[1]),
            (self.reveal.upcast_ref(), hidden[2]),
            (self.cover.upcast_ref(), hidden[3]),
        ] {
            widget.update_state(&[gtk4::accessible::State::Hidden(hidden)]);
        }
    }

    pub(super) fn touch(&self) {
        self.active.set(Some(Instant::now()));
        if self.idle.borrow().is_none() {
            self.arm_cover(self.timing().soft);
        }
    }

    fn arm_cover(&self, after: Duration) {
        let stage = self.obj().downgrade();
        let source = glib::timeout_add_local_once(after, move || {
            let Some(stage) = stage.upgrade() else {
                return;
            };
            let imp = stage.imp();
            imp.idle.take();
            if imp.awake.get() {
                imp.cover_when_idle();
            }
        });
        self.idle.replace(Some(source));
    }

    fn cover_when_idle(&self) {
        let quiet = self.active.get().map_or(Duration::MAX, |at| at.elapsed());
        let timing = self.timing();
        let limit = match self.prompt.is_empty() && !self.sheet.is_open() {
            true => timing.soft,
            false => timing.cap,
        };
        if self.prompt.is_busy() {
            return self.arm_cover(timing.soft);
        }
        match limit.checked_sub(quiet) {
            Some(remaining) if !remaining.is_zero() => self.arm_cover(remaining),
            _ => self.set_awake(false),
        }
    }

    pub(super) fn moved(&self, x: f64, y: f64) {
        if self.awake.get() {
            self.touch();
            return;
        }
        if !self.interactive.get() {
            return;
        }
        match self.origin.get() {
            None => self.origin.set(Some((x, y))),
            Some((ox, oy)) if (x - ox).hypot(y - oy) > WAKE_DISTANCE => self.set_awake(true),
            Some(_) => {}
        }
    }

    pub(super) fn ensure_blurred(&self) {
        let Some(texture) = self.background.paintable().and_downcast::<gdk::Texture>() else {
            return;
        };
        if self.blurred.paintable().is_some() {
            return;
        }
        let Some(renderer) = self.obj().native().and_then(|native| native.renderer()) else {
            return;
        };
        let Some(blurred) = crate::raster::blurred_to(&renderer, &texture, BLUR_RADIUS, BLUR_WIDTH)
        else {
            return;
        };
        self.blurred.set_paintable(Some(&blurred));
    }

    pub(super) fn dismisses(&self, target: Option<&gtk4::Widget>) -> bool {
        if !self.sheet.is_open() {
            return false;
        }
        let Some(target) = target else {
            return true;
        };
        let power = self.status.imp().power_button.get();
        let inside = |owner: &gtk4::Widget| target == owner || target.is_ancestor(owner);
        !(inside(self.sheet.upcast_ref()) || inside(power.upcast_ref()))
    }
}

#[glib::object_subclass]
impl ObjectSubclass for LockStage {
    const NAME: &'static str = "LockStage";
    type Type = super::LockStage;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        StatusIsland::static_type();
        LockClock::static_type();
        PasswordPrompt::static_type();
        TrackCard::static_type();
        SessionSheet::static_type();
        Indicator::static_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LockStage {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: LazyLock<Vec<glib::subclass::Signal>> = LazyLock::new(|| {
            vec![
                glib::subclass::Signal::builder("session-action")
                    .param_types([String::static_type()])
                    .build(),
            ]
        });
        &SIGNALS
    }

    fn constructed(&self) {
        self.parent_constructed();
        let stage = self.obj();

        stage.set_focusable(true);
        self.sync_accessible();
        stage.set_color(&gdk::RGBA::BLACK);
        self.overlay.set_measure_overlay(&*self.layout, true);
        self.corner.set_measure_overlay(&*self.cover, true);
        self.status.set_session_available(self.sheet.has_actions());
        self.status
            .imp()
            .power_button
            .bind_property("visible", &*self.cover_power, "visible")
            .sync_create()
            .build();
        self.interactive.set(true);

        self.status.connect_session_toggled(glib::clone!(
            #[weak]
            stage,
            move |_| stage.imp().sheet.toggle()
        ));
        self.sheet.connect_closed(glib::clone!(
            #[weak]
            stage,
            move |_| {
                let imp = stage.imp();
                if !imp.awake.get() {
                    return;
                }
                if !imp.prompt.grab_focus() {
                    imp.status.imp().power_button.grab_focus();
                }
            }
        ));
        self.sheet.connect_action_requested(glib::clone!(
            #[weak]
            stage,
            move |_, action| stage.emit_by_name::<()>("session-action", &[&action])
        ));

        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed(glib::clone!(
            #[weak]
            stage,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |keys, key, _, state| stage.imp().key_pressed(keys, key, state)
        ));
        stage.add_controller(keys);

        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion(glib::clone!(
            #[weak]
            stage,
            move |_, x, y| stage.imp().moved(x, y)
        ));
        stage.add_controller(motion);

        let outside = gtk4::GestureClick::new();
        outside.set_button(0);
        outside.set_propagation_phase(gtk4::PropagationPhase::Capture);
        outside.connect_pressed(glib::clone!(
            #[weak]
            stage,
            move |gesture, _, x, y| {
                let imp = stage.imp();
                if !imp.awake.get() {
                    if imp.interactive.get() {
                        gesture.set_state(gtk4::EventSequenceState::Claimed);
                        imp.set_awake(true);
                    }
                    return;
                }
                imp.touch();
                let target = stage.pick(x, y, gtk4::PickFlags::DEFAULT);
                if imp.dismisses(target.as_ref()) {
                    gesture.set_state(gtk4::EventSequenceState::Claimed);
                    imp.sheet.close();
                }
            }
        ));
        stage.add_controller(outside);
    }

    fn dispose(&self) {
        if let Some(source) = self.idle.take() {
            source.remove();
        }
        self.dispose_template();
    }
}

impl WidgetImpl for LockStage {
    fn realize(&self) {
        self.parent_realize();
        self.ensure_blurred();
    }

    fn map(&self) {
        self.parent_map();
        self.ensure_blurred();
    }
}
