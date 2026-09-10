mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Row, none_if_empty};

pub use imp::Layout;

glib::wrapper! {
    pub struct KeyboardPopover(ObjectSubclass<imp::KeyboardPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for KeyboardPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyboardPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_layouts(&self, layouts: &[Layout]) {
        let imp = self.imp();
        if imp.layouts_data.borrow().as_slice() == layouts {
            return;
        }
        imp.layouts_data.replace(layouts.to_vec());
        self.render();
    }

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_activated<F: Fn(&Self, u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |popover: Self, index: u32| f(&popover, index)),
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

    fn render(&self) {
        let imp = self.imp();
        let layouts = imp.layouts_data.borrow();
        let mut rows = imp.widgets.borrow_mut();

        for (index, layout) in layouts.iter().enumerate() {
            if rows.len() == index {
                let row = self.build_row(index as u32);
                imp.rows.append(&row);
                rows.push(row);
            }
            let row = &rows[index];
            row.set_title(none_if_empty(&layout.name));
            row.set_value(none_if_empty(&layout.code));
            row.set_selectable(true);
            row.set_selected(layout.active);
        }

        for row in rows.split_off(layouts.len()) {
            row.unparent();
        }
    }

    fn build_row(&self, index: u32) -> Row {
        let row = Row::new();
        row.set_selectable(true);
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = popover)]
            self,
            move |_| {
                popover.select(index);
                popover.emit_by_name::<()>("activated", &[&index]);
            }
        ));
        row
    }

    fn select(&self, index: u32) {
        let imp = self.imp();
        for (i, row) in imp.widgets.borrow().iter().enumerate() {
            row.set_selected(i as u32 == index);
        }
    }
}
