mod imp;

use glib::closure_local;
use gtk4::{gdk, glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    pub struct MediaArtwork(ObjectSubclass<imp::MediaArtwork>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl MediaArtwork {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_paintable(&self, paintable: Option<&gdk::Texture>) {
        let imp = self.imp();
        match paintable {
            Some(texture) => {
                imp.picture.set_paintable(Some(texture));
                imp.picture.set_visible(true);
                imp.fallback_icon.set_visible(false);
            }
            None => {
                imp.picture.set_paintable(gdk::Paintable::NONE);
                imp.picture.set_visible(false);
                imp.fallback_icon.set_visible(true);
            }
        }
    }

    pub fn set_fallback_icon_name(&self, name: &str) {
        self.imp().fallback_icon.set_icon_name(Some(name));
    }

    pub fn set_fallback_icon_pixel_size(&self, size: i32) {
        self.imp().fallback_icon.set_pixel_size(size);
    }

    pub fn set_artwork_size(&self, size: i32) {
        self.imp().size.set(size.max(1));
        self.queue_resize();
    }

    pub fn connect_activated(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "activated",
            false,
            closure_local!(move |artwork: &Self| f(artwork)),
        )
    }
}

impl Default for MediaArtwork {
    fn default() -> Self {
        Self::new()
    }
}
