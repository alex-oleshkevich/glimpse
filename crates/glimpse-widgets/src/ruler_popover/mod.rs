mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{ActionRow, Row, none_if_empty, reconcile};

pub use imp::HistoryEntry;

glib::wrapper! {
    pub struct RulerPopover(ObjectSubclass<imp::RulerPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for RulerPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl RulerPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_history(&self, entries: &[HistoryEntry]) {
        let imp = self.imp();
        if imp.history_data.borrow().as_slice() == entries {
            return;
        }
        imp.history_data.replace(entries.to_vec());
        self.render_history();
    }

    pub fn set_footer(&self, label: Option<&str>) {
        let footer = &self.imp().footer;
        footer.upcast_ref::<Row>().set_title(label);
        footer.set_visible(label.is_some());
    }

    pub fn connect_activated<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |popover: Self, id: u64| f(&popover, id)),
        )
    }

    pub fn connect_measure_requested<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "measure-requested",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    fn render_history(&self) {
        let imp = self.imp();
        #[cfg(test)]
        imp.renders.set(imp.renders.get() + 1);
        let entries = imp.history_data.borrow();
        reconcile::by_key(
            &*imp.history_rows,
            &mut imp.history_held.borrow_mut(),
            &entries,
            |entry| entry.id,
            |entry| self.build_history_row(entry),
            |row, entry| self.dress_history_row(row, entry),
        );
        imp.history_section.set_visible(!entries.is_empty());
    }

    fn build_history_row(&self, entry: &HistoryEntry) -> ActionRow {
        let row = ActionRow::new();
        let id = entry.id;
        row.connect_activated(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("activated", &[&id])
        ));
        row
    }

    fn dress_history_row(&self, row: &ActionRow, entry: &HistoryEntry) {
        let row = row.upcast_ref::<Row>();
        row.set_title(none_if_empty(&entry.title));
        row.set_value(none_if_empty(&entry.value));
    }
}
