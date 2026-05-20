mod imp;

use glib::closure_local;
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::widgets::choice_tile::ChoiceTile;

glib::wrapper! {
    pub struct ChoiceList(ObjectSubclass<imp::ChoiceList>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl ChoiceList {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn add_choice(
        &self,
        id: &str,
        primary: &str,
        secondary: Option<&str>,
        icon_name: Option<&str>,
    ) {
        let row = ChoiceTile::new();
        row.set_primary(primary);
        row.set_secondary(secondary);
        if let Some(name) = icon_name {
            row.set_left(Some(gtk4::Image::from_icon_name(name)));
        }

        let list = self.downgrade();
        let id_owned = id.to_owned();
        row.connect_activated(move |_| {
            if let Some(list) = list.upgrade() {
                list.activate_choice(&id_owned);
            }
        });

        self.append(&row);
        self.imp().rows.borrow_mut().push((id.to_owned(), row));
    }

    pub fn set_active(&self, id: &str) {
        for (row_id, row) in self.imp().rows.borrow().iter() {
            row.set_selected(row_id == id);
        }
        *self.imp().active.borrow_mut() = Some(id.to_owned());
    }

    pub fn clear_choices(&self) {
        while let Some(child) = self.first_child() {
            self.remove(&child);
        }
        self.imp().rows.borrow_mut().clear();
        *self.imp().active.borrow_mut() = None;
    }

    pub fn active(&self) -> Option<String> {
        self.imp().active.borrow().clone()
    }

    pub fn connect_changed(&self, f: impl Fn(&Self, &str) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "changed",
            false,
            closure_local!(move |list: &Self, id: String| f(list, &id)),
        )
    }

    fn activate_choice(&self, id: &str) {
        self.emit_by_name::<()>("changed", &[&id.to_owned()]);
    }
}

impl Default for ChoiceList {
    fn default() -> Self {
        Self::new()
    }
}
