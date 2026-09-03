use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{
    AccessibleRole, CompositeTemplate, TemplateChild, glib, prelude::*, subclass::prelude::*,
};

use crate::{Section, SplitRow};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/workspace_section.ui")]
pub struct WorkspaceSection {
    #[template_child]
    pub section: TemplateChild<Section>,
    #[template_child]
    pub rows: TemplateChild<gtk4::Box>,
    pub held: RefCell<Vec<(u64, SplitRow)>>,
}

#[glib::object_subclass]
impl ObjectSubclass for WorkspaceSection {
    const NAME: &'static str = "WorkspaceSection";
    type Type = super::WorkspaceSection;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(AccessibleRole::Group);
    }

    fn instance_init(object: &glib::subclass::InitializingObject<Self>) {
        object.init_template();
    }
}

impl ObjectImpl for WorkspaceSection {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("activated")
                    .param_types([u64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("details")
                    .param_types([u64::static_type()])
                    .build(),
            ]
        })
    }

    fn dispose(&self) {
        self.held.borrow_mut().clear();
        self.dispose_template();
    }
}

impl WidgetImpl for WorkspaceSection {}
