use gtk4::{CompositeTemplate, TemplateChild, glib, subclass::prelude::*};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/camera_indicator.ui")]
pub struct CameraIndicator {
    #[template_child]
    pub icon: TemplateChild<gtk4::Image>,
}

#[glib::object_subclass]
impl ObjectSubclass for CameraIndicator {
    const NAME: &'static str = "CameraIndicator";
    type Type = super::CameraIndicator;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for CameraIndicator {}
impl WidgetImpl for CameraIndicator {}
impl BoxImpl for CameraIndicator {}
