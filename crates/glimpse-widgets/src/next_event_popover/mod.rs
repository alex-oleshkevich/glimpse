mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Event, Fact};

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

    pub fn set_countdown(&self, countdown: Option<(&str, &str)>) {
        let readout = &self.imp().countdown;
        readout.set_value(countdown.map(|(value, _)| value));
        readout.set_unit(countdown.map(|(_, unit)| unit));
        readout.set_visible(countdown.is_some());
    }

    pub fn set_upcoming(&self, events: &[Event]) {
        let imp = self.imp();
        imp.later.set_events(events);
        imp.upcoming.set_visible(!events.is_empty());
    }

    pub fn set_join(&self, join: Option<(&str, &str, &str)>) {
        let imp = self.imp();
        imp.join_url.replace(join.map(|(_, _, url)| url.to_owned()));
        imp.join.set_title(join.map(|(title, _, _)| title));
        imp.join.set_subtitle(join.map(|(_, subtitle, _)| subtitle));
        imp.join.set_visible(join.is_some());
    }

    pub fn set_facts(&self, facts: &[Fact]) {
        let imp = self.imp();
        imp.facts.set_facts(facts);
        imp.details.set_visible(!facts.is_empty());
    }

    pub fn connect_join_activated<F: Fn(&Self, String) + 'static>(
        &self,
        handler: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "join-activated",
            false,
            glib::closure_local!(move |popover: Self, url: String| handler(&popover, url)),
        )
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
