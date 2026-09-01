use gtk4::{AccessibleRole, CompositeTemplate, TemplateChild, glib, subclass::prelude::*};
use std::cell::RefCell;

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/hero.ui")]
pub struct Hero {
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub text: TemplateChild<gtk4::Box>,
    #[template_child]
    pub title: TemplateChild<gtk4::Label>,
    #[template_child]
    pub subtitle: TemplateChild<gtk4::Label>,
    #[template_child]
    pub slot: TemplateChild<gtk4::Box>,
    pub gicon: RefCell<Option<gio::Icon>>,
}

#[glib::object_subclass]
impl ObjectSubclass for Hero {
    const NAME: &'static str = "GlimpseHero";
    type Type = super::Hero;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for Hero {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Hero {}
