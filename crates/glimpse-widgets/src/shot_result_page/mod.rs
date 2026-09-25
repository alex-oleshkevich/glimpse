mod imp;

use adw::prelude::*;
use gtk4::{gdk, glib, subclass::prelude::*};

use crate::ShotToolbar;

const NAME_MAX: usize = 200;

glib::wrapper! {
    pub struct ShotResultPage(ObjectSubclass<imp::ShotResultPage>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ShotResultPage {
    fn default() -> Self {
        Self::new()
    }
}

impl ShotResultPage {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn toolbar(&self) -> ShotToolbar {
        self.imp().toolbar.get()
    }

    pub fn picture(&self) -> gtk4::Picture {
        self.imp().picture.get()
    }

    pub fn set_texture(&self, texture: Option<&gdk::Texture>) {
        let picture = &self.imp().picture;
        if same_texture(picture, texture) {
            return;
        }
        picture.set_paintable(texture);
    }

    pub fn set_size_text(&self, text: &str) {
        let heading = &self.imp().heading;
        if heading.subtitle().as_str() == text {
            return;
        }
        heading.set_subtitle(text);
    }

    pub fn set_name(&self, name: &str) {
        let name: String = name.chars().take(NAME_MAX).collect();
        let row = &self.imp().name;
        if row.text().as_str() != name {
            row.set_text(&name);
        }
        self.set_heading_name(&name);
    }

    pub fn name(&self) -> String {
        self.imp().committed_name.borrow().clone()
    }

    pub(crate) fn set_heading_name(&self, name: &str) {
        let name: String = name.chars().take(NAME_MAX).collect();
        self.imp().committed_name.replace(name.clone());
        let title = format!("{name}.png");
        if self.imp().heading.title().as_str() != title {
            self.imp().heading.set_title(&title);
        }
    }

    pub(crate) fn apply_name(&self, name: &str) {
        let old = self.name();
        self.set_heading_name(name);
        if self.name() != old {
            self.emit_by_name::<()>("name-changed", &[]);
        }
    }

    pub fn set_collision(&self, collision: bool) {
        let imp = self.imp();
        if imp.colliding.get() == collision {
            return;
        }
        imp.colliding.set(collision);
        if collision {
            imp.name.add_css_class("error");
        } else {
            imp.name.remove_css_class("error");
        }
        imp.collision.set_visible(collision);
    }

    pub fn set_menu_model(&self, model: Option<&gtk4::gio::MenuModel>) {
        let button = &self.imp().menu;
        let same = match (button.menu_model(), model) {
            (Some(current), Some(next)) => &current == next,
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        button.set_menu_model(model);
        let visible = model.is_some();
        if button.get_visible() != visible {
            button.set_visible(visible);
        }
    }

    pub fn set_zoom(&self, zoom: &str) {
        if !matches!(zoom, "fit" | "actual") {
            return;
        }
        let zoom_group = &self.imp().zoom;
        if zoom_group.active_name().as_deref() == Some(zoom) {
            return;
        }
        zoom_group.set_active_name(Some(zoom));
    }

    pub fn set_can_undo(&self, can: bool) {
        let button = &self.imp().undo;
        if button.is_sensitive() == can {
            return;
        }
        button.set_sensitive(can);
    }

    pub fn set_can_redo(&self, can: bool) {
        let button = &self.imp().redo;
        if button.is_sensitive() == can {
            return;
        }
        button.set_sensitive(can);
    }

    pub fn add_toast(&self, toast: &adw::Toast) {
        self.imp().toasts.add_toast(toast.clone());
    }

    fn apply_zoom(&self) {
        let imp = self.imp();
        let fit = imp.zoom.active_name().as_deref() != Some("actual");
        let picture = &imp.picture;
        if picture.can_shrink() != fit {
            picture.set_can_shrink(fit);
        }
        if picture.hexpands() != fit {
            picture.set_hexpand(fit);
        }
        if picture.vexpands() != fit {
            picture.set_vexpand(fit);
        }
    }

    pub fn connect_copy<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "copy",
            false,
            glib::closure_local!(move |page: Self| f(&page)),
        )
    }

    pub fn connect_save<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "save",
            false,
            glib::closure_local!(move |page: Self| f(&page)),
        )
    }

    pub fn connect_open_with<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "open-with",
            false,
            glib::closure_local!(move |page: Self| f(&page)),
        )
    }

    pub fn connect_save_as<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "save-as",
            false,
            glib::closure_local!(move |page: Self| f(&page)),
        )
    }

    pub fn connect_name_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "name-changed",
            false,
            glib::closure_local!(move |page: Self| f(&page)),
        )
    }

    pub fn connect_new<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "new",
            false,
            glib::closure_local!(move |page: Self| f(&page)),
        )
    }
}

fn same_texture(picture: &gtk4::Picture, texture: Option<&gdk::Texture>) -> bool {
    let next = texture.map(|texture| texture.clone().upcast::<gdk::Paintable>());
    picture.paintable() == next
}
