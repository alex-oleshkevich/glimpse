use std::cell::RefCell;

use gtk4::{AccessibleRole, CompositeTemplate, TemplateChild, gdk, glib, subclass::prelude::*};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/notification_image_body.ui")]
pub struct NotificationImageBody {
    #[template_child]
    pub picture: TemplateChild<gtk4::Picture>,
    pub image: RefCell<Option<gdk::Texture>>,
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationImageBody {
    const NAME: &'static str = "NotificationImageBody";
    type Type = super::NotificationImageBody;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Img);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for NotificationImageBody {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for NotificationImageBody {}
