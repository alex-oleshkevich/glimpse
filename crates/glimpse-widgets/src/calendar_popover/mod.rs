mod imp;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::{Event, Ymd, Zone};

const MAX_ROWS: u32 = 4;

glib::wrapper! {
    pub struct CalendarPopover(ObjectSubclass<imp::CalendarPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for CalendarPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl CalendarPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_heading(&self, title: &str, subtitle: Option<&str>) {
        let imp = self.imp();
        imp.hero.set_title(Some(title));
        imp.hero.set_subtitle(subtitle);
    }

    pub fn selected(&self) -> Option<Ymd> {
        self.imp().calendar.selected()
    }

    pub fn open_on(&self, today: Ymd) {
        let calendar = &self.imp().calendar;
        calendar.set_today(today);
        calendar.select(today);
    }

    pub fn set_markers(&self, markers: &[(Ymd, Vec<gdk::RGBA>)]) {
        self.imp().calendar.set_events(markers);
    }

    pub fn set_day(&self, title: &str, events: &[Event]) {
        let imp = self.imp();

        if imp.day.title().as_deref() != Some(title) {
            imp.events.fold();
        }
        imp.day.set_title(Some(title));
        imp.day.set_empty(events.is_empty());
        imp.events.set_events(events);
        self.sync_day();
    }

    pub fn set_day_truncated(&self, truncated: bool) {
        if self.imp().day_truncated.replace(truncated) != truncated {
            self.sync_day();
        }
    }

    fn sync_day(&self) {
        let imp = self.imp();
        let visible = !imp.day.empty() || imp.day_truncated.get();
        if imp.day.get_visible() != visible {
            imp.day.set_visible(visible);
        }
    }

    pub fn shown_month(&self) -> (i32, u32) {
        self.imp().calendar.shown()
    }

    pub fn connect_month_shown<F: Fn(&Self, i32, u32) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "month-shown",
            false,
            glib::closure_local!(move |popover: Self, year: i32, month: u32| {
                handler(&popover, year, month);
            }),
        )
    }

    pub fn set_zones(&self, zones: &[Zone]) {
        let imp = self.imp();
        imp.zones.set_visible(!zones.is_empty());
        imp.clocks.set_zones(zones);
    }

    pub fn set_now(&self, now: &glib::DateTime) {
        self.imp().clocks.set_now(now);
    }

    pub fn set_twelve_hour(&self, twelve_hour: bool) {
        self.imp().clocks.set_twelve_hour(twelve_hour);
    }

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_day_selected<F: Fn(&Self, Ymd) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "day-selected",
            false,
            glib::closure_local!(move |popover: Self, year: i32, month: u32, day: u32| {
                handler(&popover, Ymd::new(year, month, day));
            }),
        )
    }

    pub fn connect_link_activated<F: Fn(&Self, String) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "link-activated",
            false,
            glib::closure_local!(move |popover: Self, url: String| handler(&popover, url)),
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
