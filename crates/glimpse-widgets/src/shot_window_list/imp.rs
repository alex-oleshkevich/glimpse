use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use super::ShotWindowEntry;

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/shot_window_list.ui")]
pub struct ShotWindowList {
    #[template_child]
    pub this_section: TemplateChild<gtk4::Label>,
    #[template_child]
    pub this_rows: TemplateChild<gtk4::Box>,
    #[template_child]
    pub other_section: TemplateChild<gtk4::Label>,
    #[template_child]
    pub other_rows: TemplateChild<gtk4::Box>,

    pub windows: RefCell<Vec<ShotWindowEntry>>,
}

#[glib::object_subclass]
impl ObjectSubclass for ShotWindowList {
    const NAME: &'static str = "ShotWindowList";
    type Type = super::ShotWindowList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BinLayout>();
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::List);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ShotWindowList {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("window-picked")
                    .param_types([u64::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.connect_map(move |list| list.focus_current());
    }

    fn dispose(&self) {
        self.dispose_template();
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for ShotWindowList {}
