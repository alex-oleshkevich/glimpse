use gtk4::{CompositeTemplate, TemplateChild, gdk, glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::OnceLock;

use crate::set_text;

use super::clock;

const MINUS: &str = "\u{2212}";
const STEP: f64 = 5.0;
const PAGE: f64 = 30.0;

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::Scrubber)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/scrubber.ui")]
pub struct Scrubber {
    #[template_child]
    pub track: TemplateChild<gtk4::Scale>,
    #[template_child]
    pub elapsed: TemplateChild<gtk4::Label>,
    #[template_child]
    pub remaining: TemplateChild<gtk4::Label>,

    pub(crate) held: Cell<Option<f64>>,

    #[property(name = "position", get = Self::position, set = Self::set_position)]
    position: PhantomData<f64>,
    #[property(name = "duration", get = Self::duration, set = Self::set_duration)]
    duration: PhantomData<f64>,
    #[property(name = "seekable", get = Self::seekable, set = Self::set_seekable)]
    seekable: Cell<bool>,
}

impl Scrubber {
    fn position(&self) -> f64 {
        self.track.value()
    }

    fn set_position(&self, seconds: f64) {
        if self.held.get().is_some() {
            return;
        }
        if self.position() == seconds {
            return;
        }
        self.track.set_value(seconds);
    }

    fn duration(&self) -> f64 {
        self.track.adjustment().upper()
    }

    fn set_duration(&self, seconds: f64) {
        let seconds = seconds.max(0.0);
        if self.duration() == seconds {
            return;
        }
        self.track.adjustment().set_upper(seconds);
        self.track.set_visible(seconds > 0.0);
        self.sync_times();
    }

    fn seekable(&self) -> bool {
        self.seekable.get()
    }

    fn set_seekable(&self, seekable: bool) {
        if self.seekable.replace(seekable) == seekable {
            return;
        }
        self.track.set_sensitive(seekable);
    }

    fn sync_times(&self) {
        let position = self.position();
        let duration = self.duration();
        set_text(
            &self.elapsed,
            (duration > 0.0 || position > 0.0)
                .then(|| clock(position))
                .as_deref(),
        );
        set_text(
            &self.remaining,
            (duration > 0.0)
                .then(|| format!("{MINUS}{}", clock(duration - position)))
                .as_deref(),
        );
    }

    fn emit_seek(&self) {
        self.obj().emit_by_name::<()>("seek", &[&self.position()]);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Scrubber {
    const NAME: &'static str = "Scrubber";
    type Type = super::Scrubber;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(gtk4::AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for Scrubber {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("seek")
                    .param_types([f64::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj().clone();

        let adjustment = self.track.adjustment();
        adjustment.set_step_increment(STEP);
        adjustment.set_page_increment(PAGE);

        let pointer = gtk4::EventControllerLegacy::new();
        pointer.set_propagation_phase(gtk4::PropagationPhase::Capture);
        pointer.connect_event(glib::clone!(
            #[weak]
            obj,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, event| {
                let imp = obj.imp();
                match event.event_type() {
                    gdk::EventType::ButtonPress => imp.held.set(Some(imp.position())),
                    gdk::EventType::ButtonRelease => match imp.held.take() {
                        Some(start) if start != imp.position() => imp.emit_seek(),
                        _ => {}
                    },
                    _ => {}
                }
                glib::Propagation::Proceed
            }
        ));
        obj.add_controller(pointer);

        obj.connect_unmap(|scrubber| scrubber.imp().held.set(None));

        self.track.connect_value_changed(glib::clone!(
            #[weak]
            obj,
            move |_| obj.imp().sync_times()
        ));

        self.track.connect_change_value(glib::clone!(
            #[weak]
            obj,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, _, value| {
                let imp = obj.imp();
                if imp.held.get().is_none() && value != imp.position() {
                    imp.track.set_value(value);
                    imp.emit_seek();
                }
                glib::Propagation::Proceed
            }
        ));

        self.sync_times();
    }

    fn dispose(&self) {
        self.dispose_template();
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for Scrubber {}
