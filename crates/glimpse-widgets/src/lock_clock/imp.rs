use gtk4::{CompositeTemplate, TemplateChild, glib, subclass::prelude::*};
use std::cell::RefCell;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/lock_clock.ui")]
pub struct LockClock {
    #[template_child]
    pub time: TemplateChild<gtk4::Label>,
    #[template_child]
    pub date: TemplateChild<gtk4::Label>,
    pub time_format: RefCell<String>,
    pub date_format: RefCell<String>,
}

#[glib::object_subclass]
impl ObjectSubclass for LockClock {
    const NAME: &'static str = "LockClock";
    type Type = super::LockClock;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LockClock {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for LockClock {}
