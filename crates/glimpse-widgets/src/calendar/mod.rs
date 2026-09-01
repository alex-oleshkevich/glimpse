mod dots;
mod grid;
mod imp;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

pub use grid::Ymd;

glib::wrapper! {
    pub struct Calendar(ObjectSubclass<imp::Calendar>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Calendar {
    fn default() -> Self {
        Self::new()
    }
}

impl Calendar {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn today(&self) -> Ymd {
        self.imp().today.get()
    }

    pub fn set_today(&self, today: Ymd) {
        let imp = self.imp();
        if imp.today.get() == today {
            return;
        }
        imp.today.set(today);
        imp.render();
    }

    pub fn selected(&self) -> Option<Ymd> {
        self.imp().selected.get()
    }

    pub fn select(&self, date: Ymd) {
        self.imp().select(date);
    }

    pub fn shown(&self) -> (i32, u32) {
        self.imp().shown.get()
    }

    pub fn show_month(&self, year: i32, month: u32) {
        let imp = self.imp();
        imp.shown.set((year, month.clamp(1, 12)));
        imp.render();
    }

    pub fn step(&self, by: i32) {
        self.imp().step(by);
    }

    pub fn clear_selection(&self) {
        let imp = self.imp();
        if imp.selected.get().is_none() {
            return;
        }
        imp.selected.set(None);
        imp.render();
    }

    pub fn set_events(&self, events: &[(Ymd, Vec<gdk::RGBA>)]) {
        let imp = self.imp();
        let mut stored = imp.events.borrow_mut();
        stored.clear();
        for (date, colors) in events {
            stored.insert(*date, colors.iter().copied().take(dots::MAX).collect());
        }
        drop(stored);
        imp.render();
    }

    pub fn events(&self, date: Ymd) -> Vec<gdk::RGBA> {
        self.imp()
            .events
            .borrow()
            .get(&date)
            .cloned()
            .unwrap_or_default()
    }

    pub fn connect_day_selected<F: Fn(&Self, Ymd) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "day-selected",
            false,
            glib::closure_local!(move |calendar: Self, year: i32, month: u32, day: u32| {
                handler(&calendar, Ymd::new(year, month, day));
            }),
        )
    }
}
