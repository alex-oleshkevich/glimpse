use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};

use super::Workspace;

#[derive(Debug, Default)]
pub struct WorkspaceList {
    pub workspaces: RefCell<Vec<Workspace>>,
    pub sections: RefCell<Vec<crate::Section>>,
}

#[glib::object_subclass]
impl ObjectSubclass for WorkspaceList {
    const NAME: &'static str = "WorkspaceList";
    type Type = super::WorkspaceList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::Group);
    }
}

impl ObjectImpl for WorkspaceList {
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

    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        list.add_css_class("workspace-list");
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
    }

    fn dispose(&self) {
        for section in self.sections.borrow_mut().drain(..) {
            section.unparent();
        }
    }
}

impl WidgetImpl for WorkspaceList {}
