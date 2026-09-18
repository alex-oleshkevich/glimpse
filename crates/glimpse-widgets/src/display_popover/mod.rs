mod imp;

use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::{Display, drawer, set_footer_row};

const ENABLE_REQUESTED: &str = "enable-requested";
const BLANKED: &str = "blanked";
const FOOTER_ACTIVATED: &str = "footer-activated";

glib::wrapper! {
    pub struct DisplayPopover(ObjectSubclass<imp::DisplayPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for DisplayPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_displays(&self, displays: &[Display]) {
        let imp = self.imp();
        if imp.entries.borrow().as_slice() == displays {
            return;
        }
        imp.entries.replace(displays.to_vec());
        self.render();
    }

    pub fn set_output_power(&self, supported: bool) {
        let imp = self.imp();
        if imp.output_power.get() == supported {
            return;
        }
        imp.output_power.set(supported);
        self.render();
    }

    pub fn set_footer(&self, label: Option<&str>) {
        set_footer_row(&self.imp().footer, label);
    }

    pub fn connect_enable_requested<F: Fn(&Self, &str, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            ENABLE_REQUESTED,
            false,
            glib::closure_local!(move |popover: Self, connector: String, enabled: bool| f(
                &popover, &connector, enabled
            )),
        )
    }

    pub fn connect_blanked<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            BLANKED,
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            FOOTER_ACTIVATED,
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }

    fn render(&self) {
        let imp = self.imp();
        let displays = imp.entries.borrow().clone();
        let power = imp.output_power.get();

        imp.section.set_visible(!displays.is_empty());
        imp.devices.set_output_power(power);
        imp.devices.set_displays(&displays);
        imp.blank.set_visible(power && !displays.is_empty());
    }

    fn set_details_open(&self, open: bool) {
        let imp = self.imp();
        crate::set_css_class(&*imp.hero, drawer::RECEDED, open);
        crate::set_css_class(&*imp.blank, drawer::RECEDED, open);
        crate::set_css_class(&*imp.footer, drawer::RECEDED, open);
    }
}
