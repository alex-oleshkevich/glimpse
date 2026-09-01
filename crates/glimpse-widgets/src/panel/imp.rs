use gtk4::{AccessibleRole, CompositeTemplate, TemplateChild, glib, subclass::prelude::*};
use std::cell::Cell;

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/panel.ui")]
pub struct Panel {
    pub thickness: Cell<i32>,
    #[template_child]
    pub container: TemplateChild<gtk4::CenterBox>,
    #[template_child]
    pub start_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub center_box: TemplateChild<gtk4::Box>,
    #[template_child]
    pub end_box: TemplateChild<gtk4::Box>,
}

#[glib::object_subclass]
impl ObjectSubclass for Panel {
    const NAME: &'static str = "Panel";
    type Type = super::Panel;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Toolbar);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for Panel {
    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for Panel {}
