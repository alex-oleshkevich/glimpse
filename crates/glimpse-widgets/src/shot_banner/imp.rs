use std::cell::Cell;

use gtk4::{CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/shot_banner.ui")]
pub struct ShotBanner {
    #[template_child]
    pub body: TemplateChild<gtk4::Label>,
    #[template_child]
    pub keys: TemplateChild<gtk4::Label>,

    pub portal: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for ShotBanner {
    const NAME: &'static str = "ShotBanner";
    type Type = super::ShotBanner;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
        klass.bind_template();
        klass.set_accessible_role(gtk4::AccessibleRole::Group);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ShotBanner {
    fn dispose(&self) {
        self.dispose_template();
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for ShotBanner {}
