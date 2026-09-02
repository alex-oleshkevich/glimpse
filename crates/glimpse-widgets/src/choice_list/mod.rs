mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::Row;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    pub detail: String,
    pub icon_name: String,
}

glib::wrapper! {
    pub struct ChoiceList(ObjectSubclass<imp::ChoiceList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ChoiceList {
    fn default() -> Self {
        Self::new()
    }
}

impl ChoiceList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_choices(&self, choices: &[Choice]) {
        let imp = self.imp();
        if imp.choices.borrow().as_slice() == choices {
            return;
        }
        imp.choices.replace(choices.to_vec());
        imp.selected.set(None);
        self.render();
    }

    pub fn selected(&self) -> Option<u32> {
        self.imp().selected.get()
    }

    pub fn set_selected(&self, selected: Option<u32>) {
        let imp = self.imp();
        let selected = selected.filter(|index| (*index as usize) < imp.choices.borrow().len());
        if imp.selected.replace(selected) == selected {
            return;
        }
        self.sync_selection();
    }

    pub fn connect_activated<F: Fn(&Self, u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |list: Self, index: u32| f(&list, index)),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        let choices = imp.choices.borrow();
        let mut rows = imp.rows.borrow_mut();

        for (index, choice) in choices.iter().enumerate() {
            if rows.len() == index {
                let row = self.build_row(index as u32);
                row.insert_after(self, rows.last());
                rows.push(row);
            }
            let row = &rows[index];
            row.set_title(none_if_empty(&choice.label));
            row.set_subtitle(none_if_empty(&choice.detail));
            row.set_lead_icon(none_if_empty(&choice.icon_name));
        }

        for row in rows.split_off(choices.len()) {
            row.unparent();
        }

        drop(rows);
        drop(choices);
        self.sync_selection();
    }

    fn build_row(&self, index: u32) -> Row {
        let row = Row::new();
        row.set_selectable(true);
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| {
                list.set_selected(Some(index));
                list.emit_by_name::<()>("activated", &[&index]);
            }
        ));
        row
    }

    fn sync_selection(&self) {
        let imp = self.imp();
        let selected = imp.selected.get();
        for (index, row) in imp.rows.borrow().iter().enumerate() {
            row.set_selected(selected == Some(index as u32));
        }
    }
}

fn none_if_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}
