use gtk4::{CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

#[derive(CompositeTemplate, Default)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/date_hero.ui")]
pub struct DateHero {
    #[template_child]
    pub(super) weekday: TemplateChild<gtk4::Label>,
    #[template_child]
    pub(super) date: TemplateChild<gtk4::Label>,
}

#[glib::object_subclass]
impl ObjectSubclass for DateHero {
    const NAME: &'static str = "DateHero";
    type Type = super::DateHero;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for DateHero {}
impl WidgetImpl for DateHero {}
impl BoxImpl for DateHero {}
