mod imp;

use std::path::Path;

use gtk4::{glib, prelude::*, subclass::prelude::*};

glib::wrapper! {
    /// Round, fixed-size avatar widget. Shows a face image scaled to the
    /// widget's logical pixel size when a path is set, otherwise falls back
    /// to the configured initials. Content is clipped to the rounded border
    /// from CSS via `overflow: hidden`.
    pub struct Avatar(ObjectSubclass<imp::Avatar>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl Avatar {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Set the avatar's logical pixel size. The widget is square; size
    /// drives both width and height. Default is 96.
    pub fn set_size(&self, size: i32) {
        self.imp().overlay.set_size_request(size, size);
    }

    /// Set the initials shown when no image is available. Empty string
    /// hides the label.
    pub fn set_initials(&self, text: &str) {
        let initials = &self.imp().initials;
        initials.set_label(text);
        // Keep the label hidden while a face image is shown; the image's
        // visibility state is the source of truth.
        if !self.imp().picture.is_visible() {
            initials.set_visible(!text.is_empty());
        }
    }

    /// Set the face image. None clears it and falls back to initials.
    /// The pixbuf is loaded at the widget's current size so the Picture's
    /// natural size matches the avatar's footprint (gtk::Picture reports
    /// the paintable's intrinsic pixel dimensions as its natural size).
    pub fn set_path(&self, path: Option<&Path>) {
        let imp = self.imp();
        let Some(path) = path else {
            imp.picture.set_paintable(gtk4::gdk::Paintable::NONE);
            imp.picture.set_visible(false);
            imp.initials.set_visible(!imp.initials.label().is_empty());
            return;
        };
        let size = self.imp().overlay.width_request().max(1);
        match gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(path, size, size, true) {
            Ok(pixbuf) => {
                // Texture::for_pixbuf is deprecated since GTK 4.20 in favour of
                // gdk::MemoryTexture, but the replacement requires extracting
                // the raw byte buffer and stride manually. Acceptable for a
                // single user-icon load.
                #[allow(deprecated)]
                let texture = gtk4::gdk::Texture::for_pixbuf(&pixbuf);
                imp.picture.set_paintable(Some(&texture));
                imp.picture.set_visible(true);
                imp.initials.set_visible(false);
            }
            Err(error) => {
                tracing::debug!(%error, path = %path.display(), "Avatar: failed to load face icon");
                imp.picture.set_paintable(gtk4::gdk::Paintable::NONE);
                imp.picture.set_visible(false);
                imp.initials.set_visible(!imp.initials.label().is_empty());
            }
        }
    }
}

impl Default for Avatar {
    fn default() -> Self {
        Self::new()
    }
}
