mod imp;

use gettextrs::gettext;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, none_if_empty, reconcile};

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

    pub fn set_latest(&self, latest: Option<(&str, &str)>) {
        let imp = self.imp();
        match latest {
            Some((title, subtitle)) => {
                imp.hero.set_title(Some(title));
                imp.hero.set_subtitle(Some(subtitle));
            }
            None => {
                imp.hero
                    .set_title(Some(gettext("No measurement yet").as_str()));
                imp.hero.set_subtitle(Some(
                    gettext("Right-click the indicator to measure").as_str(),
                ));
            }
        }
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
        let row = &self.imp().footer;
        row.set_title(none_if_empty(label.unwrap_or_default()));
        row.set_visible(label.is_some());
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

    fn build_history_row(&self, entry: &HistoryEntry) -> Row {
        let row = Row::new();
        row.add_css_class("ruler-row");
        row.set_activatable(true);
        let id = entry.id;
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("activated", &[&id])
        ));
        row
    }

    fn dress_history_row(&self, row: &Row, entry: &HistoryEntry) {
        row.set_title(none_if_empty(&entry.title));
        row.set_value(none_if_empty(&entry.value));
    }
}
