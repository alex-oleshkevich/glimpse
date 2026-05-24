use gtk4::{CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseLock/widgets/avatar.ui")]
pub struct Avatar {
    #[template_child]
    pub overlay: TemplateChild<gtk4::Overlay>,
    #[template_child]
    pub picture: TemplateChild<gtk4::Picture>,
    #[template_child]
    pub initials: TemplateChild<gtk4::Label>,
}

#[glib::object_subclass]
impl ObjectSubclass for Avatar {
    const NAME: &'static str = "Avatar";
    type Type = super::Avatar;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for Avatar {}
impl WidgetImpl for Avatar {}
impl BoxImpl for Avatar {}
