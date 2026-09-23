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
mod tests {
    use super::*;

    fn group(key: &str, app: &str, count: u32) -> ChipGroup {
        ChipGroup {
            key: key.to_owned(),
            app: app.to_owned(),
            icon: None,
            count,
        }
    }

    #[test]
    fn an_app_name_is_cleaned_before_the_chip_label() {
        assert_eq!(chip_label("Slack", 1), "Slack: 1 new notification");
        assert_eq!(chip_label("Slack", 3), "Slack: 3 new notifications");

        let hostile = "  Some   App\n\n".repeat(20);
        let name = chip_label(&hostile, 2);
        assert!(
            !name.contains('\n'),
            "the app name is flattened to one line"
        );
        assert!(name.ends_with(": 2 new notifications"));
        let app_part = name.strip_suffix(": 2 new notifications").unwrap();
        assert_eq!(
            app_part.chars().count(),
            APP_MAX_CHARS + 1,
            "an unbounded app name is capped by characters, with a trailing ellipsis"
        );

        let bidi = chip_label("Lunch\u{202e}gpj.exe", 1);
        assert!(
            !bidi.contains('\u{202e}'),
            "a bidi override does not reach the chip label"
        );
    }

    fn chip_at(chips: &NotificationChips, index: usize) -> gtk4::Box {
        let mut child = chips.first_child();
        for _ in 0..index {
            child = child.and_then(|widget| widget.next_sibling());
        }
        child
            .and_downcast::<gtk4::Box>()
            .unwrap_or_else(|| panic!("no chip at {index}"))
    }

    #[test]
    #[ignore = "needs a display"]
    fn notification_chips_states() {
        if gtk4::init().is_err() {
            return;
        }
        crate::register_resources().expect("resources");

        let chips = NotificationChips::new();
        let window = gtk4::Window::new();
        window.set_child(Some(&chips));

        assert!(!chips.get_visible(), "an untouched strip starts hidden");

        chips.set_groups(&[]);
        assert!(!chips.get_visible(), "an empty list keeps it hidden");

        chips.set_groups(&[group("slack", "Slack", 3), group("mail", "Mail", 0)]);
        assert!(chips.get_visible(), "at least one nonzero group shows it");
        assert_eq!(
            chips.observe_children().n_items(),
            1,
            "a zero-count group is skipped entirely"
        );

        let slack = chip_at(&chips, 0);
        let icon_widget = slack
            .first_child()
            .and_downcast::<gtk4::Image>()
            .expect("icon");
        assert!(
            !icon_widget.get_visible(),
            "a None icon reserves nothing on a fresh chip"
        );
        let count_label = slack
            .last_child()
            .and_downcast::<gtk4::Label>()
            .expect("count label");
        assert_eq!(count_label.text().as_str(), "3");
        assert_eq!(
            slack.tooltip_text().as_deref(),
            Some("Slack: 3 new notifications"),
            "the tooltip carries the same string as the accessible label"
        );

        chips.set_groups(&[
            group("slack", "Slack", 3),
            ChipGroup {
                key: "camera".to_owned(),
                app: "Camera".to_owned(),
                icon: Some(gio::ThemedIcon::new("camera-photo-symbolic").upcast()),
                count: 1,
            },
        ]);
        let camera = chip_at(&chips, 1);
        let camera_icon = camera
            .first_child()
            .and_downcast::<gtk4::Image>()
            .expect("icon");
        assert!(camera_icon.get_visible(), "a Some icon shows the icon slot");

        chips.set_groups(&[group("mail", "Mail", 1), group("slack", "Slack", 5)]);
        assert_eq!(chips.observe_children().n_items(), 2);
        assert_eq!(
            chip_at(&chips, 1),
            slack,
            "the reordered slack chip reuses its own widget rather than rebuilding it"
        );
        assert_eq!(
            chip_at(&chips, 0).tooltip_text().as_deref(),
            Some("Mail: 1 new notification")
        );

        chips.set_groups(&[group("slack", "Slack", 5)]);
        assert_eq!(
            chips.observe_children().n_items(),
            1,
            "a dropped key removes its chip"
        );
        assert_eq!(chip_at(&chips, 0), slack, "the surviving chip is unchanged");

        chips.set_groups(&[]);
        assert!(!chips.get_visible(), "an empty list hides the strip again");
        assert_eq!(chips.observe_children().n_items(), 0);

        window.destroy();
    }
}
