use gtk4::{AccessibleRole, CompositeTemplate, TemplateChild, glib, subclass::prelude::*};
use std::cell::{Cell, RefCell};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/indicator.ui")]
pub struct Indicator {
    #[template_child]
    pub dot: TemplateChild<crate::dots::Dots>,
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub attention_dot: TemplateChild<gtk4::Box>,
    #[template_child]
    pub label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub badge: TemplateChild<gtk4::Label>,
    pub gicon: RefCell<Option<gio::Icon>>,
    pub color: Cell<Option<gtk4::gdk::RGBA>>,
    pub attention: Cell<bool>,
    pub severity: Cell<Option<crate::Severity>>,
}

#[glib::object_subclass]
impl ObjectSubclass for Indicator {
    const NAME: &'static str = "Indicator";
    type Type = super::Indicator;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Generic);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for Indicator {
    fn constructed(&self) {
        self.parent_constructed();
        self.dot.set_max(1);
        self.dot.set_size(super::DOT_SIZE);
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Indicator {}
