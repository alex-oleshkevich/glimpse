mod imp;

use glib::closure_local;
use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct Message(ObjectSubclass<imp::Message>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl Message {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_icon(&self, icon: Option<&str>) {
        let imp = self.imp();
        imp.icon.set_icon_name(icon);
        imp.icon.set_visible(icon.is_some());
    }

    pub fn set_content_paintable(&self, paintable: Option<&impl IsA<gdk::Paintable>>) {
        let imp = self.imp();
        imp.content_icon.set_paintable(paintable);
        imp.content_icon.set_visible(paintable.is_some());
    }

    pub fn set_app_name(&self, name: &str) {
        let imp = self.imp();
        imp.app_name.set_text(name);
        imp.app_name.set_visible(!name.is_empty());
    }

    pub fn set_time(&self, time: &str) {
        self.imp().time_label.set_text(time);
    }

    pub fn set_title(&self, title: &str) {
        self.imp().title.set_text(title);
    }

    pub fn set_body(&self, body: &str) {
        let imp = self.imp();
        imp.body_label.set_text(body);
        imp.body_label.set_visible(!body.is_empty());
    }

    pub fn add_action(&self, id: &str, label: &str) {
        let imp = self.imp();
        let btn = gtk4::Button::with_label(label);
        btn.add_css_class("message__action");
        btn.add_css_class("flat");
        let obj = self.downgrade();
        let id = id.to_owned();
        btn.connect_clicked(move |_| {
            if let Some(w) = obj.upgrade() {
                w.emit_by_name::<()>("action", &[&id]);
            }
        });
        imp.actions.append(&btn);
        imp.actions.set_visible(true);
    }

    pub fn clear_actions(&self) {
        let imp = self.imp();
        while let Some(child) = imp.actions.first_child() {
            imp.actions.remove(&child);
        }
        imp.actions.set_visible(false);
    }

    pub fn connect_closed(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure("closed", false, closure_local!(move |w: &Self| f(w)))
    }

    pub fn connect_clicked(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure("clicked", false, closure_local!(move |w: &Self| f(w)))
    }

    pub fn connect_secondary_clicked(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "secondary-clicked",
            false,
            closure_local!(move |w: &Self| f(w)),
        )
    }

    pub fn connect_action(&self, f: impl Fn(&Self, &str) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "action",
            false,
            closure_local!(move |w: &Self, id: String| f(w, &id)),
        )
    }
}

impl Default for Message {
    fn default() -> Self {
        Self::new()
    }
}
