use gtk4::{CompositeTemplate, TemplateChild, gdk, glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;
use std::sync::OnceLock;

use crate::{LockClock, NotificationChips, PasswordPrompt, SessionSheet, StatusIsland, TrackCard};

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
    pub scrim: TemplateChild<gtk4::Box>,
    #[template_child]
    pub layout: TemplateChild<gtk4::CenterBox>,
    #[template_child]
    pub status: TemplateChild<StatusIsland>,
    #[template_child]
    pub clock: TemplateChild<LockClock>,
    #[template_child]
    pub prompt: TemplateChild<PasswordPrompt>,
    #[template_child]
    pub chips: TemplateChild<NotificationChips>,
    #[template_child]
    pub track: TemplateChild<TrackCard>,
    #[template_child]
    pub sheet: TemplateChild<SessionSheet>,

    pub dim: Cell<f64>,
    pub color: Cell<Option<gdk::RGBA>>,
}

impl LockStage {
    fn key_pressed(&self, key: gdk::Key) -> glib::Propagation {
        if key != gdk::Key::Escape || !self.sheet.is_open() {
            return glib::Propagation::Proceed;
        }
        self.sheet.close();
        glib::Propagation::Stop
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
        NotificationChips::static_type();
        TrackCard::static_type();
        SessionSheet::static_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LockStage {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("session-action")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let stage = self.obj();

        stage.set_color(&gdk::RGBA::BLACK);
        self.overlay.set_measure_overlay(&*self.layout, true);
        self.status.set_session_available(self.sheet.has_actions());

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
            move |_, key, _, _| stage.imp().key_pressed(key)
        ));
        stage.add_controller(keys);

        let outside = gtk4::GestureClick::new();
        outside.set_button(0);
        outside.set_propagation_phase(gtk4::PropagationPhase::Capture);
        outside.connect_pressed(glib::clone!(
            #[weak]
            stage,
            move |gesture, _, x, y| {
                let imp = stage.imp();
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
        self.dispose_template();
    }
}

impl WidgetImpl for LockStage {}
