use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};

use super::{ACTION_INVOKED, ACTIVATED, DISMISSED, Notification};
use crate::NotificationCard;

const SPACING: u32 = 6;

#[derive(Debug, Default)]
pub struct NotificationList {
    pub notifications: RefCell<Vec<Notification>>,
    pub rows: RefCell<Vec<(String, NotificationCard)>>,
    pub cap: Cell<Option<usize>>,
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationList {
    const NAME: &'static str = "NotificationList";
    type Type = super::NotificationList;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::Group);
    }
}

impl ObjectImpl for NotificationList {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder(ACTIVATED)
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder(DISMISSED)
                    .param_types([String::static_type()])
                    .build(),
                glib::subclass::Signal::builder(ACTION_INVOKED)
                    .param_types([String::static_type(), String::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        let list = self.obj();
        list.add_css_class("notification-list");
        list.set_visible(false);
        if let Some(layout) = list.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(gtk4::Orientation::Vertical);
            layout.set_spacing(SPACING);
        }
    }

    fn dispose(&self) {
        for (_, row) in self.rows.borrow_mut().drain(..) {
            row.unparent();
        }
    }
}

impl WidgetImpl for NotificationList {}
