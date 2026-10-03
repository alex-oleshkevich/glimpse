use gtk4::{CompositeTemplate, TemplateChild, accessible, glib, prelude::*, subclass::prelude::*};
use std::cell::Cell;

#[derive(Debug, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::MeterRow)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/meter_row.ui")]
pub struct MeterRow {
    #[template_child]
    pub bar: TemplateChild<gtk4::ProgressBar>,
    #[property(
        name = "level",
        get,
        set = Self::set_level,
        default = -1.0,
        explicit_notify,
        builder().lax_validation()
    )]
    level: Cell<f64>,
}

impl Default for MeterRow {
    fn default() -> Self {
        Self {
            bar: TemplateChild::default(),
            level: Cell::new(-1.0),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for MeterRow {
    const NAME: &'static str = "MeterRow";
    type Type = super::MeterRow;
    type ParentType = crate::Row;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(gtk4::AccessibleRole::ProgressBar);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for MeterRow {
    fn constructed(&self) {
        self.parent_constructed();
        self.sync_bar(self.level.get());
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for MeterRow {}

impl crate::row::RowImpl for MeterRow {}

impl MeterRow {
    fn set_level(&self, level: f64) {
        let level = clamp_level(level);
        if self.level.get() == level {
            return;
        }
        self.level.set(level);
        self.sync_bar(level);
        self.obj().notify_level();
    }

    fn sync_bar(&self, level: f64) {
        let shown = level >= 0.0;
        if shown && (self.bar.fraction() - level).abs() > f64::EPSILON {
            self.bar.set_fraction(level);
        }
        if self.bar.get_visible() != shown {
            self.bar.set_visible(shown);
        }
        crate::set_css_class(&*self.obj(), "meter-row--bar", shown);
        let announced = if shown { level } else { 0.0 };
        self.obj().update_property(&[
            accessible::Property::ValueMin(0.0),
            accessible::Property::ValueMax(1.0),
            accessible::Property::ValueNow(announced),
        ]);
    }
}

fn clamp_level(level: f64) -> f64 {
    if !level.is_finite() || level < 0.0 {
        -1.0
    } else if level > 1.0 {
        1.0
    } else {
        level
    }
}
