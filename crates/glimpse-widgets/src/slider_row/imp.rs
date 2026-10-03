use gtk4::{
    CompositeTemplate, TemplateChild, accessible, gdk, glib, prelude::*, subclass::prelude::*,
};
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::sync::OnceLock;

use crate::{RowTone, TEXT_MAX_CHARS, set_css_class, truncate};

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::SliderRow)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/slider_row.ui")]
pub struct SliderRow {
    #[template_child]
    pub controls: TemplateChild<gtk4::Box>,
    #[template_child]
    pub start: TemplateChild<gtk4::Box>,
    #[template_child]
    pub scale: TemplateChild<gtk4::Scale>,
    #[template_child]
    pub subtitle: TemplateChild<gtk4::Label>,
    owner_value: Cell<f64>,
    pub(crate) held: Cell<bool>,
    quiet: Cell<bool>,
    #[property(get = Self::title, set = Self::set_title, nullable, explicit_notify)]
    title: RefCell<Option<String>>,
    #[property(name = "value", get = Self::value, set = Self::set_value, minimum = f64::NEG_INFINITY, maximum = f64::INFINITY, explicit_notify, lax_validation)]
    value_prop: PhantomData<f64>,
    #[property(name = "min", get = Self::min, set = Self::set_min, explicit_notify, lax_validation)]
    min_prop: PhantomData<f64>,
    #[property(name = "max", get = Self::max, set = Self::set_max, default = 1.0, explicit_notify, lax_validation)]
    max_prop: PhantomData<f64>,
    #[property(name = "step", get = Self::step, set = Self::set_step, default = 0.01, explicit_notify, lax_validation)]
    step_prop: PhantomData<f64>,
    #[property(name = "tone", get, set = Self::set_tone, builder(RowTone::Normal).explicit_notify())]
    tone: Cell<RowTone>,
    #[property(name = "inactive", get, set = Self::set_inactive, explicit_notify)]
    inactive: Cell<bool>,
    #[property(name = "subtitle", get = Self::subtitle, set = Self::set_subtitle, nullable, explicit_notify)]
    subtitle_prop: PhantomData<Option<String>>,
    #[property(name = "tooltip", get = Self::tooltip, set = Self::set_tooltip, nullable, explicit_notify)]
    tooltip_prop: PhantomData<Option<String>>,
}

impl SliderRow {
    fn title(&self) -> Option<String> {
        self.title.borrow().clone()
    }
    fn set_title(&self, value: Option<String>) {
        let value = value
            .map(|text| truncate(&text, TEXT_MAX_CHARS))
            .filter(|text| !text.is_empty());
        if self.title() == value {
            return;
        }
        *self.title.borrow_mut() = value;
        self.sync_accessible();
        self.obj().notify_title();
    }
    fn value(&self) -> f64 {
        self.scale.value()
    }
    fn set_value(&self, value: f64) {
        if !value.is_finite() {
            return;
        }
        let value = value.clamp(self.min(), self.max());
        if self.held.get() {
            return;
        }
        if self.value() == value {
            return;
        }
        self.quiet.set(true);
        self.owner_value.set(value);
        self.scale.set_value(value);
        self.quiet.set(false);
        self.obj().notify_value();
    }
    fn min(&self) -> f64 {
        self.scale.adjustment().lower()
    }
    fn set_min(&self, value: f64) {
        if !value.is_finite() || self.min() == value {
            return;
        }
        let adjustment = self.scale.adjustment();
        if adjustment.upper() < value {
            adjustment.set_upper(value);
            self.obj().notify_max();
        }
        adjustment.set_lower(value);
        self.obj().notify_min();
        self.set_value(self.value().clamp(self.min(), self.max()));
    }
    fn max(&self) -> f64 {
        self.scale.adjustment().upper()
    }
    fn set_max(&self, value: f64) {
        if !value.is_finite() {
            return;
        }
        let value = value.max(self.min());
        if self.max() == value {
            return;
        }
        self.scale.adjustment().set_upper(value);
        self.obj().notify_max();
        self.set_value(self.value().clamp(self.min(), self.max()));
    }
    fn step(&self) -> f64 {
        self.scale.adjustment().step_increment()
    }
    fn set_step(&self, value: f64) {
        if !value.is_finite() || value <= 0.0 || self.step() == value {
            return;
        }
        let adjustment = self.scale.adjustment();
        adjustment.set_step_increment(value);
        adjustment.set_page_increment(value);
        self.obj().notify_step();
    }
    fn set_tone(&self, value: RowTone) {
        if self.tone.replace(value) == value {
            return;
        }
        let row = self.obj();
        set_css_class(
            row.upcast_ref::<gtk4::Widget>(),
            "row--warning",
            value == RowTone::Warning,
        );
        set_css_class(
            row.upcast_ref::<gtk4::Widget>(),
            "row--error",
            value == RowTone::Error,
        );
        set_css_class(
            row.upcast_ref::<gtk4::Widget>(),
            "row--destructive",
            value == RowTone::Destructive,
        );
        row.notify_tone();
    }
    fn set_inactive(&self, value: bool) {
        if self.inactive.replace(value) == value {
            return;
        }
        if value {
            self.held.set(false);
            let controllers = self.scale.observe_controllers();
            for index in 0..controllers.n_items() {
                if let Some(controller) = controllers
                    .item(index)
                    .and_downcast::<gtk4::EventController>()
                {
                    controller.reset();
                }
            }
        }
        set_css_class(&*self.obj(), "row--inactive", value);
        if self.scale.is_sensitive() == value {
            self.scale.set_sensitive(!value);
        }
        self.obj().notify_inactive();
    }
    fn subtitle(&self) -> Option<String> {
        let text = self.subtitle.text();
        (!text.is_empty()).then(|| text.to_string())
    }
    fn set_subtitle(&self, value: Option<String>) {
        let text = truncate(value.as_deref().unwrap_or(""), TEXT_MAX_CHARS);
        if self.subtitle.text().as_str() == text {
            return;
        }
        self.subtitle.set_text(&text);
        if self.subtitle.get_visible() == text.is_empty() {
            self.subtitle.set_visible(!text.is_empty());
        }
        self.sync_accessible();
        self.obj().notify_subtitle();
    }
    fn sync_accessible(&self) {
        self.scale.update_property(&[
            accessible::Property::Label(self.title().as_deref().unwrap_or("")),
            accessible::Property::Description(self.subtitle().as_deref().unwrap_or("")),
        ]);
    }
    fn tooltip(&self) -> Option<String> {
        self.obj().tooltip_text().map(|value| value.to_string())
    }
    fn set_tooltip(&self, value: Option<String>) {
        let value = value
            .map(|text| truncate(&text, TEXT_MAX_CHARS))
            .filter(|text| !text.is_empty());
        if self.tooltip() == value {
            return;
        }
        self.obj().set_tooltip_text(value.as_deref());
        self.obj().notify_tooltip();
    }
    pub(crate) fn finish_drag(&self) {
        if self.held.take() && !self.inactive.get() {
            let value = self.value();
            self.obj().emit_by_name::<()>("changed", &[&value]);
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for SliderRow {
    const NAME: &'static str = "SliderRow";
    type Type = super::SliderRow;
    type ParentType = gtk4::Widget;
    type Interfaces = (gtk4::Buildable,);

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_css_name("listrow");
        klass.set_accessible_role(gtk4::AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for SliderRow {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("moved")
                    .param_types([f64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("changed")
                    .param_types([f64::static_type()])
                    .build(),
            ]
        })
    }
    fn constructed(&self) {
        self.parent_constructed();
        let row = self.obj().clone();
        self.set_step(0.01);
        self.scale.adjustment().connect_value_changed(glib::clone!(
            #[weak]
            row,
            move |adjustment| {
                let imp = row.imp();
                if imp.quiet.get() {
                    return;
                }
                if imp.inactive.get() {
                    let value = imp.owner_value.get().clamp(imp.min(), imp.max());
                    imp.quiet.set(true);
                    if adjustment.value() != value {
                        adjustment.set_value(value);
                    }
                    imp.quiet.set(false);
                    return;
                }
                imp.owner_value.set(adjustment.value());
                row.notify_value();
                if imp.held.get() {
                    row.emit_by_name::<()>("moved", &[&adjustment.value()]);
                }
            }
        ));
        let pointer = gtk4::EventControllerLegacy::new();
        pointer.set_propagation_phase(gtk4::PropagationPhase::Capture);
        pointer.connect_event(glib::clone!(
            #[weak]
            row,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, event| {
                let imp = row.imp();
                match event.event_type() {
                    gdk::EventType::ButtonPress if !imp.inactive.get() => imp.held.set(true),
                    gdk::EventType::ButtonRelease => imp.finish_drag(),
                    _ => {}
                }
                glib::Propagation::Proceed
            }
        ));
        self.scale.add_controller(pointer);
        row.connect_unmap(|row| {
            row.imp().held.set(false);
        });
        self.scale.connect_change_value(glib::clone!(
            #[weak]
            row,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, _, value| {
                let imp = row.imp();
                if imp.inactive.get() {
                    return glib::Propagation::Stop;
                }
                if !imp.held.get() && value.is_finite() {
                    let value = value.clamp(imp.min(), imp.max());
                    if value != imp.value() {
                        imp.set_value(value);
                        row.emit_by_name::<()>("changed", &[&value]);
                    }
                }
                glib::Propagation::Proceed
            }
        ));
    }
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for SliderRow {}

impl BuildableImpl for SliderRow {
    fn add_child(&self, builder: &gtk4::Builder, child: &glib::Object, type_: Option<&str>) {
        let own_template = self.start.try_get().is_none();
        match (type_, child.downcast_ref::<gtk4::Widget>()) {
            _ if own_template => self.parent_add_child(builder, child, type_),
            (Some("start"), Some(widget)) => self.obj().set_start(widget),
            _ => self.parent_add_child(builder, child, type_),
        }
    }
}
