use gtk4::{CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/screencast_indicator.ui")]
pub struct ScreenCastIndicator {
    #[template_child]
    pub rec_icon: TemplateChild<gtk4::Image>,
    #[template_child]
    pub timer_label: TemplateChild<gtk4::Label>,
}

#[glib::object_subclass]
impl ObjectSubclass for ScreenCastIndicator {
    const NAME: &'static str = "ScreenCastIndicator";
    type Type = super::ScreenCastIndicator;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ScreenCastIndicator {}
impl WidgetImpl for ScreenCastIndicator {}
impl BoxImpl for ScreenCastIndicator {}
