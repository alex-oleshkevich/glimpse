use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, accessible, glib, prelude::*,
    subclass::prelude::*,
};
use std::marker::PhantomData;

use crate::set_text;

#[derive(Debug, Default, CompositeTemplate, glib::Properties)]
#[properties(wrapper_type = super::Readout)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/readout.ui")]
pub struct Readout {
    #[template_child]
    pub value: TemplateChild<gtk4::Label>,
    #[template_child]
    pub unit: TemplateChild<gtk4::Label>,

    #[property(name = "value", get = Self::value, set = Self::set_value, nullable)]
    value_text: PhantomData<Option<String>>,
    #[property(name = "unit", get = Self::unit, set = Self::set_unit, nullable)]
    unit_text: PhantomData<Option<String>>,
}

impl Readout {
    fn value(&self) -> Option<String> {
        self.value
            .get_visible()
            .then(|| self.value.text().to_string())
    }

    fn set_value(&self, value: Option<String>) {
        set_text(&self.value, value.as_deref());
        self.sync_accessible_label();
    }

    fn unit(&self) -> Option<String> {
        self.unit
            .get_visible()
            .then(|| self.unit.text().to_string())
    }

    fn set_unit(&self, unit: Option<String>) {
        set_text(&self.unit, unit.as_deref());
        self.sync_accessible_label();
    }

    fn sync_accessible_label(&self) {
        let label = format!("{}{}", self.value.text(), self.unit.text());
        self.obj()
            .update_property(&[accessible::Property::Label(&label)]);
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Readout {
    const NAME: &'static str = "Readout";
    type Type = super::Readout;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

#[glib::derived_properties]
impl ObjectImpl for Readout {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Readout {}
