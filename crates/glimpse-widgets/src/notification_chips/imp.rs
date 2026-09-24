use gettextrs::gettext;
use gtk4::{AccessibleRole, glib, prelude::*, subclass::prelude::*};
use std::cell::RefCell;

#[derive(Debug, Default)]
pub struct NotificationChips {
    pub chips: RefCell<Vec<(String, gtk4::Box)>>,
}

#[glib::object_subclass]
impl ObjectSubclass for NotificationChips {
    const NAME: &'static str = "NotificationChips";
    type Type = super::NotificationChips;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.set_layout_manager_type::<gtk4::BoxLayout>();
        klass.set_accessible_role(AccessibleRole::Group);
    }
}

impl ObjectImpl for NotificationChips {
    fn constructed(&self) {
        self.parent_constructed();
        let widget = self.obj();
        widget.add_css_class("notification-chips");
        widget.set_visible(false);
        widget.update_property(&[gtk4::accessible::Property::Label(&gettext("Notifications"))]);

        if let Some(layout) = widget.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_spacing(8);
        }
    }

    fn dispose(&self) {
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for NotificationChips {}
