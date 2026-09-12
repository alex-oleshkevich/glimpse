mod imp;

use gtk4::{gio, glib, prelude::*, subclass::prelude::*};

const DISMISSED: &str = "dismissed";

glib::wrapper! {
    pub struct NotificationHeader(ObjectSubclass<imp::NotificationHeader>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for NotificationHeader {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationHeader {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_app_icon(&self, icon: Option<&gio::Icon>) {
        let imp = self.imp();
        if crate::icons_equal(imp.gicon.borrow().as_ref(), icon) {
            return;
        }
        imp.gicon.replace(icon.cloned());
        match icon {
            Some(icon) => imp.app_icon.set_from_gicon(icon),
            None => imp.app_icon.clear(),
        }
        imp.app_icon.set_visible(icon.is_some());
    }

    pub fn set_controls_visible(&self, visible: bool) {
        self.imp().close.set_visible(visible);
    }

    pub(crate) fn set_dismiss_label(&self, label: &str) {
        self.imp()
            .close
            .update_property(&[gtk4::accessible::Property::Label(label)]);
    }

    pub fn connect_dismissed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            DISMISSED,
            false,
            glib::closure_local!(move |header: &Self| f(header)),
        )
    }
}
