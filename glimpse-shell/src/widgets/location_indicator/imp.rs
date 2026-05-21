use gtk4::{CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/location_indicator.ui")]
pub struct LocationIndicator {
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
}

#[glib::object_subclass]
impl ObjectSubclass for LocationIndicator {
    const NAME: &'static str = "LocationIndicator";
    type Type = super::LocationIndicator;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LocationIndicator {}
impl WidgetImpl for LocationIndicator {}
impl BoxImpl for LocationIndicator {}
