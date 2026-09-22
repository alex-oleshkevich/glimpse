#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;

use gtk4::{AccessibleRole, CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

use crate::{Hero, Notice, Placeholder, PopoverShell, Row, Section};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Usage {
    pub id: String,
    pub icon: String,
    pub title: String,
    pub detail: Option<String>,
}

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/privacy_popover.ui")]
pub struct PrivacyPopover {
    #[template_child]
    pub shell: TemplateChild<PopoverShell>,
    #[template_child]
    pub hero: TemplateChild<Hero>,
    #[template_child]
    pub screen_notice: TemplateChild<Notice>,
    #[template_child]
    pub usages: TemplateChild<Section>,
    #[template_child]
    pub empty_usages: TemplateChild<Placeholder>,
    #[template_child]
    pub usage_rows: TemplateChild<gtk4::Box>,

    pub usage_data: RefCell<Vec<Usage>>,
    pub usage_held: RefCell<Vec<(String, Row)>>,
    #[cfg(test)]
    pub renders: Cell<u32>,
}

#[glib::object_subclass]
impl ObjectSubclass for PrivacyPopover {
    const NAME: &'static str = "PrivacyPopover";
    type Type = super::PrivacyPopover;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for PrivacyPopover {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for PrivacyPopover {}
