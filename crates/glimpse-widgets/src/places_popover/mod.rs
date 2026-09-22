mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, none_if_empty, reconcile, set_footer_row};

pub use imp::{Entry, Trash};

glib::wrapper! {
    pub struct PlacesPopover(ObjectSubclass<imp::PlacesPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for PlacesPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl PlacesPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_places(&self, entries: &[Entry]) {
        let imp = self.imp();
        if imp.places_data.borrow().as_slice() == entries {
            return;
        }
        imp.places_data.replace(entries.to_vec());
        self.render_places();
    }

    pub fn set_bookmarks(&self, entries: &[Entry]) {
        let imp = self.imp();
        if imp.bookmarks_data.borrow().as_slice() == entries {
            return;
        }
        imp.bookmarks_data.replace(entries.to_vec());
        self.render_bookmarks();
    }

    pub fn set_network(&self, shares: &[Entry]) {
        let imp = self.imp();
        if imp.network_data.borrow().as_slice() == shares {
            return;
        }
        imp.network_data.replace(shares.to_vec());
        self.render_network();
    }

    pub fn set_trash(&self, trash: Option<Trash>) {
        let imp = self.imp();
        if *imp.trash_data.borrow() == trash {
            return;
        }
        imp.trash_data.replace(trash);
        self.render_trash();
    }

    pub fn set_overflow(&self, bookmarks: Option<&str>) {
        set_footer_row(&self.imp().bookmarks_more, bookmarks);
    }

    pub fn set_footer(&self, label: Option<&str>) {
        set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_activated<F: Fn(&Self, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |popover: Self, id: String| f(&popover, &id)),
        )
    }

    pub fn connect_more<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "more",
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

    fn render_places(&self) {
        let imp = self.imp();
        let entries = imp.places_data.borrow().clone();
        imp.places.set_visible(!entries.is_empty());
        reconcile::by_key(
            &*imp.places_rows,
            &mut imp.places_held.borrow_mut(),
            &entries,
            |entry| entry.id.clone(),
            |entry| self.build_entry_row(&entry.id),
            apply_entry_row,
        );
    }

    fn render_bookmarks(&self) {
        let imp = self.imp();
        let entries = imp.bookmarks_data.borrow().clone();
        imp.bookmarks.set_visible(!entries.is_empty());
        reconcile::by_key(
            &*imp.bookmarks_rows,
            &mut imp.bookmarks_held.borrow_mut(),
            &entries,
            |entry| entry.id.clone(),
            |entry| self.build_entry_row(&entry.id),
            apply_entry_row,
        );
    }

    fn render_network(&self) {
        let imp = self.imp();
        let shares = imp.network_data.borrow().clone();
        imp.network.set_visible(!shares.is_empty());
        reconcile::by_key(
            &*imp.network_rows,
            &mut imp.network_held.borrow_mut(),
            &shares,
            |entry| entry.id.clone(),
            |entry| self.build_entry_row(&entry.id),
            apply_entry_row,
        );
    }

    fn render_trash(&self) {
        let imp = self.imp();
        let trash = *imp.trash_data.borrow();
        imp.trash.set_visible(trash.is_some());
        let Some(trash) = trash else {
            return;
        };
        let empty = trash.items == 0;
        imp.trash_row.set_lead_icon(Some(match empty {
            true => "user-trash-symbolic",
            false => "user-trash-full-symbolic",
        }));
        imp.trash_row
            .set_title(Some(gettextrs::gettext("Trash").as_str()));
        let value = match empty {
            true => gettextrs::gettext("Empty"),
            false => gettextrs::ngettext("{n} item", "{n} items", trash.items)
                .replace("{n}", &trash.items.to_string()),
        };
        imp.trash_row.set_value(Some(value.as_str()));
    }

    fn build_entry_row(&self, id: &str) -> Row {
        let row = Row::new();
        let key = id.to_owned();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| popover.emit_by_name::<()>("activated", &[&key])
        ));
        row
    }
}

fn apply_entry_row(row: &Row, entry: &Entry) {
    row.set_title(none_if_empty(&entry.title));
    row.set_subtitle(none_if_empty(&entry.subtitle));
    row.set_lead_icon(none_if_empty(&entry.icon));
    row.set_busy(entry.busy);
}
