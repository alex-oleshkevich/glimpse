mod imp;
mod row;

pub use row::EventRow;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::Row;

const OVERFLOW_ICON: &str = "go-next-symbolic";
const QUIET: &str = "row--quiet";

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Event {
    pub summary: String,
    pub detail: String,
    pub when: String,
    pub color: Option<gdk::RGBA>,
}

glib::wrapper! {
    pub struct EventList(ObjectSubclass<imp::EventList>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for EventList {
    fn default() -> Self {
        Self::new()
    }
}

impl EventList {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_events(&self, events: &[Event]) {
        let imp = self.imp();
        if imp.events.borrow().as_slice() == events {
            return;
        }
        imp.events.replace(events.to_vec());
        self.render();
    }

    pub fn set_activatable(&self, activatable: bool) {
        if self.imp().activatable.replace(activatable) == activatable {
            return;
        }
        self.render();
    }

    pub fn set_max_rows(&self, max: u32) {
        if self.imp().max_rows.replace(max) == max {
            return;
        }
        self.render();
    }

    pub fn connect_activated<F: Fn(&Self, u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |list: Self, index: u32| f(&list, index)),
        )
    }

    pub fn connect_overflow<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "overflow",
            false,
            glib::closure_local!(move |list: Self| f(&list)),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        let events = imp.events.borrow();
        let max = imp.max_rows.get() as usize;
        let shown = match max {
            0 => events.len(),
            max => events.len().min(max),
        };
        let leads = events[..shown].iter().any(|event| event.color.is_some());
        let activatable = imp.activatable.get();

        let mut rows = imp.rows.borrow_mut();
        for (index, event) in events.iter().take(shown).enumerate() {
            if rows.len() == index {
                let row = self.build_row(index as u32);
                row.insert_after(self, rows.last());
                rows.push(row);
            }
            let row = &rows[index];
            let item: &Row = row.upcast_ref();
            item.set_title(Some(event.summary.as_str()));
            item.set_subtitle(none_if_empty(&event.detail));
            item.set_activatable(activatable);
            row.set_when(none_if_empty(&event.when));
            row.set_color(event.color, leads);
        }

        for row in rows.split_off(shown) {
            row.unparent();
        }

        let hidden = events.len() - shown;
        drop(rows);
        drop(events);
        self.sync_overflow(hidden);
    }

    fn build_row(&self, index: u32) -> EventRow {
        let row = EventRow::new();
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("activated", &[&index])
        ));
        row
    }

    fn sync_overflow(&self, hidden: usize) {
        let imp = self.imp();
        if hidden == 0 {
            if let Some(row) = imp.overflow.take() {
                row.unparent();
            }
            return;
        }

        let existing = imp.overflow.borrow().clone();
        let row = match existing {
            Some(row) => row,
            None => {
                let row = self.build_overflow();
                imp.overflow.replace(Some(row.clone()));
                row
            }
        };

        row.set_title(Some(match hidden {
            1 => "1 more event".to_owned(),
            hidden => format!("{hidden} more events"),
        }));
        let rows = imp.rows.borrow();
        row.insert_after(self, rows.last());
    }

    fn build_overflow(&self) -> Row {
        let row = Row::new();
        row.add_css_class(QUIET);
        row.set_trail(&gtk4::Image::from_icon_name(OVERFLOW_ICON));
        row.connect_clicked(glib::clone!(
            #[weak(rename_to = list)]
            self,
            move |_| list.emit_by_name::<()>("overflow", &[])
        ));
        row
    }
}

fn none_if_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}
