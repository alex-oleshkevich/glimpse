use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};

use super::Clip;
use crate::Expandable;

#[derive(Debug, Default)]
pub struct ClipboardList {
    pub clips: RefCell<Vec<Clip>>,
    pub holders: RefCell<Vec<(u64, Expandable)>>,
    /// Wording for the two action rows. A widget owns structure and no content, so these arrive
    /// from the applet rather than being written here.
    pub actions: RefCell<Actions>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Actions {
    pub pin: String,
    pub unpin: String,
    pub forget: String,
}

#[glib::object_subclass]
impl ObjectSubclass for ClipboardList {
    const NAME: &'static str = "ClipboardList";
    type Type = super::ClipboardList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::List);
    }
}

impl ObjectImpl for ClipboardList {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("restored")
                    .param_types([u64::static_type()])
                    .build(),
                glib::subclass::Signal::builder("pinned")
                    .param_types([u64::static_type(), bool::static_type()])
                    .build(),
                glib::subclass::Signal::builder("removed")
                    .param_types([u64::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        list.add_css_class("clipboard-list");
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
        }
    }

    fn dispose(&self) {
        for (_, holder) in self.holders.borrow_mut().drain(..) {
            holder.unparent();
        }
    }
}

impl WidgetImpl for ClipboardList {}
