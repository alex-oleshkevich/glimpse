mod imp;

use adw::prelude::*;
use gtk4::{glib, subclass::prelude::*};

use crate::ShotToolbar;

const NAME_MAX: usize = 200;
const ZOOM_STEP: f64 = 1.25;
const ZOOM_MIN: f64 = 0.1;
const ZOOM_MAX: f64 = 8.0;

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

    pub fn scroller(&self) -> gtk4::ScrolledWindow {
        self.imp().scroller.get()
    }

    pub fn hand(&self) -> gtk4::ToggleButton {
        self.imp().hand.get()
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

    pub fn zoom_by(&self, steps: f64) {
        let imp = self.imp();
        let picture = &imp.picture;
        let Some((width, height)) = intrinsic(picture) else {
            return;
        };
        let current = imp.level.get().unwrap_or_else(|| {
            (f64::from(picture.width()) / width).min(f64::from(picture.height()) / height)
        });
        let level = (current * ZOOM_STEP.powf(steps)).clamp(ZOOM_MIN, ZOOM_MAX);
        if current <= 0.0 || (level - current).abs() < f64::EPSILON {
            return;
        }
        let ratio = level / current;
        let scroller = &imp.scroller;
        let (hadjustment, vadjustment) = (scroller.hadjustment(), scroller.vadjustment());
        let (x, y) = imp
            .pointer
            .get()
            .unwrap_or((hadjustment.page_size() / 2.0, vadjustment.page_size() / 2.0));
        let anchor = (
            (hadjustment.value() + x) * ratio - x,
            (vadjustment.value() + y) * ratio - y,
        );
        imp.level.set(Some(level));
        if imp.zoom.active() != gtk4::INVALID_LIST_POSITION {
            imp.zoom.set_active(gtk4::INVALID_LIST_POSITION);
        }
        self.apply_zoom();
        let (width, height) = picture.size_request();
        let margins = (
            picture.margin_start() + picture.margin_end(),
            picture.margin_top() + picture.margin_bottom(),
        );
        for (adjustment, extent, margin, value) in [
            (&hadjustment, width, margins.0, anchor.0),
            (&vadjustment, height, margins.1, anchor.1),
        ] {
            adjustment.set_upper(adjustment.page_size().max(f64::from(extent + margin)));
            adjustment.set_value(value);
        }
    }

    fn apply_zoom(&self) {
        let imp = self.imp();
        let name = imp.zoom.active_name();
        if name.is_some() {
            imp.level.set(None);
        }
        let picture = &imp.picture;
        let custom = imp
            .level
            .get()
            .zip(intrinsic(picture))
            .map(|(level, (width, height))| {
                (
                    (width * level).round() as i32,
                    (height * level).round() as i32,
                )
            });
        let fit = custom.is_none() && name.as_deref() != Some("actual");
        let shrink = fit || custom.is_some();
        if picture.can_shrink() != shrink {
            picture.set_can_shrink(shrink);
        }
        if picture.hexpands() != fit {
            picture.set_hexpand(fit);
        }
        if picture.vexpands() != fit {
            picture.set_vexpand(fit);
        }
        let (width, height) = custom.unwrap_or((-1, -1));
        if picture.size_request() != (width, height) {
            picture.set_size_request(width, height);
        }
        let (align, content) = match custom {
            Some(_) => (gtk4::Align::Center, gtk4::ContentFit::Contain),
            None => (gtk4::Align::Fill, gtk4::ContentFit::ScaleDown),
        };
        if picture.halign() != align {
            picture.set_halign(align);
            picture.set_valign(align);
        }
        if picture.content_fit() != content {
            picture.set_content_fit(content);
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

fn intrinsic(picture: &gtk4::Picture) -> Option<(f64, f64)> {
    let paintable = picture.paintable()?;
    let (width, height) = (paintable.intrinsic_width(), paintable.intrinsic_height());
    (width > 0 && height > 0).then(|| (f64::from(width), f64::from(height)))
}
