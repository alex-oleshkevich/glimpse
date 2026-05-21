use gtk4::{CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/mic_indicator.ui")]
pub struct MicIndicator {
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
}

#[glib::object_subclass]
impl ObjectSubclass for MicIndicator {
    const NAME: &'static str = "MicIndicator";
    type Type = super::MicIndicator;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for MicIndicator {}
impl WidgetImpl for MicIndicator {}
impl BoxImpl for MicIndicator {}
