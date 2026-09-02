mod imp;

use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

use crate::{Scrubber, Transport};

glib::wrapper! {
    pub struct NowPlaying(ObjectSubclass<imp::NowPlaying>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for NowPlaying {
    fn default() -> Self {
        Self::new()
    }
}

impl NowPlaying {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn scrubber(&self) -> Scrubber {
        self.imp().scrubber.get()
    }

    pub fn transport(&self) -> Transport {
        self.imp().transport.get()
    }

    pub fn set_art(&self, art: Option<&impl IsA<gdk::Paintable>>) {
        self.imp().set_art(art.map(|art| art.as_ref()));
    }
}
