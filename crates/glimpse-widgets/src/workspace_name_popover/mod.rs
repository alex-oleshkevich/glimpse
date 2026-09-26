mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, none_if_empty, reconcile};

glib::wrapper! {
    pub struct WorkspaceNamePopover(ObjectSubclass<imp::WorkspaceNamePopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for WorkspaceNamePopover {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceNamePopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_workspace(&self, title: &str, subtitle: &str) {
        let hero = &self.imp().hero;
        hero.set_title(none_if_empty(title));
        hero.set_subtitle(none_if_empty(subtitle));
    }

    pub fn set_name(&self, name: &str) {
        let imp = self.imp();
        if imp.given.borrow().as_str() == name {
            return;
        }
        let untouched = imp.name.text().as_str() == imp.given.borrow().as_str();
        imp.given.replace(name.to_owned());
        if untouched {
            imp.name.set_text(name);
        }
    }

    pub fn set_taken(&self, taken: &[(String, String)]) {
        let imp = self.imp();
        if imp.taken_names.borrow().as_slice() == taken {
            return;
        }
        imp.taken_names.replace(taken.to_vec());
        self.show_clash();
    }

    pub fn set_recent(&self, names: &[String]) {
        let imp = self.imp();
        if imp.recent_names.borrow().as_slice() == names {
            return;
        }
        imp.recent_names.replace(names.to_vec());
        imp.recent.set_visible(!names.is_empty());
        reconcile::by_key(
            &*imp.recent_rows,
            &mut imp.recent_held.borrow_mut(),
            names,
            |name| name.clone(),
            |name| {
                let row = Row::new();
                let name = name.clone();
                row.connect_clicked(glib::clone!(
                    #[weak(rename_to = popover)]
                    self,
                    move |_| popover.emit_by_name::<()>("submitted", &[&name])
                ));
                row
            },
            |row, name| row.set_title(none_if_empty(name)),
        );
    }

    pub(crate) fn clash(&self) -> Option<String> {
        let imp = self.imp();
        let typed = imp.name.text();
        let typed = typed.trim();
        imp.taken_names
            .borrow()
            .iter()
            .find(|(name, _)| !typed.is_empty() && name.eq_ignore_ascii_case(typed))
            .map(|(_, message)| message.clone())
    }

    pub(crate) fn show_clash(&self) {
        let imp = self.imp();
        let clash = self.clash();
        imp.taken.set_label(clash.as_deref().unwrap_or_default());
        if imp.taken.get_visible() != clash.is_some() {
            imp.taken.set_visible(clash.is_some());
        }
        crate::set_css_class(&*imp.name, "error", clash.is_some());
    }

    pub fn focus_entry(&self) {
        let entry = &self.imp().name;
        entry.grab_focus();
        entry.select_region(0, -1);
    }

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_submitted<F: Fn(&Self, String) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "submitted",
            false,
            glib::closure_local!(move |popover: Self, name: String| handler(&popover, name)),
        )
    }

    pub fn connect_cancelled<F: Fn(&Self) + 'static>(&self, handler: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "cancelled",
            false,
            glib::closure_local!(move |popover: Self| handler(&popover)),
        )
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| handler(&popover)),
        )
    }
}
