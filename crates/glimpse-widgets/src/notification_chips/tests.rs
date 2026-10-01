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
