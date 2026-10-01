mod imp;

use gettextrs::{gettext, ngettext};
use gtk4::{glib, prelude::*, subclass::prelude::*};

use crate::reconcile::by_key;

const APP_MAX_CHARS: usize = 64;

glib::wrapper! {
    pub struct NotificationChips(ObjectSubclass<imp::NotificationChips>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for NotificationChips {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ChipGroup {
    pub key: String,
    pub app: String,
    pub icon: Option<gio::Icon>,
    pub count: u32,
}

impl NotificationChips {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_groups(&self, groups: &[ChipGroup]) {
        let visible: Vec<ChipGroup> = groups
            .iter()
            .filter(|group| group.count > 0)
            .cloned()
            .collect();

        by_key(
            self,
            &mut self.imp().chips.borrow_mut(),
            &visible,
            |group| group.key.clone(),
            |_| build_chip(),
            apply_chip,
        );

        self.set_visible(!visible.is_empty());
    }
}

fn build_chip() -> gtk4::Box {
    let chip: gtk4::Box = glib::Object::builder()
        .property("accessible-role", gtk4::AccessibleRole::Img)
        .build();
    chip.set_orientation(gtk4::Orientation::Horizontal);
    chip.set_spacing(6);
    chip.add_css_class("notification-chip");

    let icon = gtk4::Image::new();
    icon.set_visible(false);
    icon.set_accessible_role(gtk4::AccessibleRole::Presentation);
    chip.append(&icon);

    let count = gtk4::Label::new(None);
    count.set_accessible_role(gtk4::AccessibleRole::Presentation);
    chip.append(&count);

    chip
}

fn apply_chip(chip: &gtk4::Box, group: &ChipGroup) {
    let Some(icon_widget) = chip.first_child().and_downcast::<gtk4::Image>() else {
        return;
    };
    let Some(count_label) = chip.last_child().and_downcast::<gtk4::Label>() else {
        return;
    };

    if !crate::icons_equal(icon_widget.gicon().as_ref(), group.icon.as_ref()) {
        match &group.icon {
            Some(icon) => icon_widget.set_from_gicon(icon),
            None => icon_widget.clear(),
        }
        icon_widget.set_visible(group.icon.is_some());
    }

    let count_text = group.count.to_string();
    if count_label.text().as_str() != count_text {
        count_label.set_text(&count_text);
    }

    let name = chip_label(&group.app, group.count);
    if chip.tooltip_text().as_deref() != Some(name.as_str()) {
        chip.set_tooltip_text(Some(&name));
        chip.update_property(&[gtk4::accessible::Property::Label(&name)]);
    }
}

fn count_phrase(count: u32) -> String {
    ngettext(
        "{count} new notification",
        "{count} new notifications",
        count,
    )
    .replace("{count}", &count.to_string())
}

fn chip_label(app: &str, count: u32) -> String {
    let app = glimpse_utils::clean(app, APP_MAX_CHARS);
    gettext("{app}: {notifications}")
        .replace("{app}", &app)
        .replace("{notifications}", &count_phrase(count))
}

#[cfg(test)]
mod tests;
