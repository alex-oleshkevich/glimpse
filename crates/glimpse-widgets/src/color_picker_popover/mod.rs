mod imp;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::{Shade, drawer, none_if_empty};

glib::wrapper! {
    pub struct ColorPickerPopover(ObjectSubclass<imp::ColorPickerPopover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ColorPickerPopover {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorPickerPopover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_latest(&self, latest: Option<(&str, gdk::RGBA)>) {
        let imp = self.imp();
        match latest {
            Some((title, color)) => {
                imp.hero.set_title(Some(title));
                imp.latest.set_color(Some(&color));
            }
            None => imp.hero.set_title(imp.resting.borrow().as_deref()),
        }
        imp.latest.set_visible(latest.is_some());
    }

    pub fn set_shades(&self, shades: &[Shade]) {
        let imp = self.imp();
        imp.palette.set_shades(shades);
        imp.palette_section.set_empty(shades.is_empty());
    }

    pub fn set_open(&self, open: Option<u64>) {
        let imp = self.imp();
        imp.palette.set_open(open);
        for chrome in [
            imp.hero.upcast_ref::<gtk4::Widget>(),
            imp.footer.upcast_ref(),
        ] {
            crate::set_css_class(chrome, drawer::RECEDED, open.is_some());
        }
    }

    pub fn set_footer(&self, label: Option<&str>) {
        let row = &self.imp().footer;
        row.set_title(none_if_empty(label.unwrap_or_default()));
        row.set_visible(label.is_some());
    }

    pub fn connect_activated<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            glib::closure_local!(move |popover: Self, id: u64| f(&popover, id)),
        )
    }

    pub fn connect_detailed<F: Fn(&Self, u64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "detailed",
            false,
            glib::closure_local!(move |popover: Self, id: u64| f(&popover, id)),
        )
    }

    pub fn connect_copied<F: Fn(&Self, u64, &str) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "copied",
            false,
            glib::closure_local!(move |popover: Self, id: u64, key: String| f(&popover, id, &key)),
        )
    }

    pub fn connect_footer_activated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "footer-activated",
            false,
            glib::closure_local!(move |popover: Self| f(&popover)),
        )
    }
}
