mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::Event;

glib::wrapper! {
    pub struct NextEventPopover(ObjectSubclass<imp::NextEventPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for NextEventPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl NextEventPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_heading(&self, title: &str, subtitle: Option<&str>) {
        let imp = self.imp();
        imp.hero.set_title(Some(title));
        imp.hero.set_subtitle(subtitle);
    }

    pub fn set_nothing(&self) {
        let imp = self.imp();
        let (title, subtitle) = imp.quiet.borrow().clone();
        imp.hero.set_title(Some(title.as_str()));
        imp.hero.set_subtitle(Some(subtitle.as_str()));
        self.set_countdown(None);
        self.set_upcoming(&[]);
    }

    pub fn set_countdown(&self, countdown: Option<(&str, &str)>) {
        let readout = &self.imp().countdown;
        readout.set_value(countdown.map(|(value, _)| value));
        readout.set_unit(countdown.map(|(_, unit)| unit));
        readout.set_visible(countdown.is_some());
    }

    pub fn set_upcoming(&self, events: &[Event]) {
        let imp = self.imp();
        imp.upcoming.set_empty(events.is_empty());
        imp.later.set_events(events);
    }

    pub fn set_footer(&self, label: Option<&str>) {
        crate::set_footer_row(&self.imp().footer, label);
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
