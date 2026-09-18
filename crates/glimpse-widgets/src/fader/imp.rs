use gtk4::{CompositeTemplate, TemplateChild, gdk, glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::OnceLock;

use super::{CHANGED, MOVED, TOGGLED};

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::Fader)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/fader.ui")]
pub struct Fader {
    #[template_child]
    pub mute: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub track: TemplateChild<gtk4::Scale>,

    pub(crate) held: Cell<Option<f64>>,
    pub(crate) quiet: Cell<bool>,

    #[property(name = "value", get = Self::value, set = Self::set_value)]
    value: PhantomData<f64>,
    #[property(name = "muted", get = Self::muted, set = Self::set_muted)]
    muted: PhantomData<bool>,
    #[property(name = "icon-name", get = Self::icon_name, set = Self::set_icon_name, nullable)]
    icon_name: PhantomData<Option<String>>,
    #[property(name = "maximum", get = Self::maximum, set = Self::set_maximum, default = 100.0)]
    maximum: PhantomData<f64>,
    #[property(name = "floor", get = Self::floor, set = Self::set_floor, default = 0.0)]
    floor: PhantomData<f64>,
    #[property(name = "toggleable", get = Self::toggleable, set = Self::set_toggleable, default = true)]
    toggleable: PhantomData<bool>,
}

impl Fader {
    fn value(&self) -> f64 {
        self.track.value()
    }

    fn set_value(&self, value: f64) {
        if self.held.get().is_some() {
            return;
        }
        let value = value.clamp(self.floor(), self.maximum());
        if self.value() == value {
            return;
        }
        self.track.set_value(value);
    }

    fn muted(&self) -> bool {
        self.mute.is_active()
    }

    fn set_muted(&self, muted: bool) {
        if self.muted() == muted {
            return;
        }
        self.quiet.set(true);
        self.mute.set_active(muted);
        self.quiet.set(false);
    }

    fn icon_name(&self) -> Option<String> {
        self.mute
            .icon_name()
            .map(|name| name.to_string())
            .filter(|name| !name.is_empty())
    }

    fn set_icon_name(&self, name: Option<String>) {
        if self.icon_name() == name {
            return;
        }
        self.mute.set_icon_name(name.as_deref().unwrap_or_default());
        self.icon.set_icon_name(name.as_deref());
    }

    fn maximum(&self) -> f64 {
        self.track.adjustment().upper()
    }

    fn set_maximum(&self, maximum: f64) {
        let maximum = maximum.max(self.floor());
        if self.maximum() == maximum {
            return;
        }
        let adjustment = self.track.adjustment();
        adjustment.set_upper(maximum);
        self.apply_increments(maximum);
        adjustment.set_value(adjustment.value());
        self.obj().notify_value();
    }

    fn apply_increments(&self, maximum: f64) {
        let adjustment = self.track.adjustment();
        adjustment.set_step_increment((maximum / 100.0).max(1.0));
        adjustment.set_page_increment((maximum / 20.0).max(1.0));
    }

    fn floor(&self) -> f64 {
        self.track.adjustment().lower()
    }

    fn set_floor(&self, floor: f64) {
        let floor = floor.max(0.0).min(self.maximum());
        if self.floor() == floor {
            return;
        }
        let adjustment = self.track.adjustment();
        adjustment.set_lower(floor);
        adjustment.set_value(adjustment.value());
        self.obj().notify_value();
    }

    fn toggleable(&self) -> bool {
        self.mute.get_visible()
    }

    fn set_toggleable(&self, toggleable: bool) {
        if self.toggleable() == toggleable {
            return;
        }
        self.mute.set_visible(toggleable);
        self.icon.set_visible(!toggleable);
    }

    fn emit_changed(&self) {
        self.obj().emit_by_name::<()>(CHANGED, &[&self.value()]);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Fader {
    const NAME: &'static str = "Fader";
    type Type = super::Fader;
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
impl ObjectImpl for Fader {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder(CHANGED)
                    .param_types([f64::static_type()])
                    .build(),
                glib::subclass::Signal::builder(TOGGLED)
                    .param_types([bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder(MOVED)
                    .param_types([f64::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj().clone();

        let adjustment = self.track.adjustment();
        self.apply_increments(adjustment.upper());
        adjustment.connect_value_changed(glib::clone!(
            #[weak]
            obj,
            move |adjustment| {
                obj.emit_by_name::<()>(MOVED, &[&adjustment.value()]);
            }
        ));

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
                    gdk::EventType::ButtonPress => imp.held.set(Some(imp.value())),
                    gdk::EventType::ButtonRelease => match imp.held.take() {
                        Some(start) if start != imp.value() => imp.emit_changed(),
                        _ => {}
                    },
                    _ => {}
                }
                glib::Propagation::Proceed
            }
        ));
        obj.add_controller(pointer);

        obj.connect_unmap(|fader| fader.imp().held.set(None));

        self.track.connect_change_value(glib::clone!(
            #[weak]
            obj,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, _, value| {
                let imp = obj.imp();
                let value = value.clamp(imp.floor(), imp.maximum());
                if imp.held.get().is_none() && value != imp.value() {
                    imp.track.set_value(value);
                    imp.emit_changed();
                }
                glib::Propagation::Proceed
            }
        ));

        self.mute.connect_active_notify(glib::clone!(
            #[weak]
            obj,
            move |mute| {
                obj.notify_muted();
                if obj.imp().quiet.get() {
                    return;
                }
                obj.emit_by_name::<()>(TOGGLED, &[&mute.is_active()]);
            }
        ));
    }

    fn dispose(&self) {
        self.dispose_template();
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for Fader {}
