use super::*;
use std::cell::Cell;
use std::rc::Rc;

fn child_named<T: IsA<gtk4::Widget>>(parent: &impl IsA<gtk4::Widget>, class: &str) -> T {
    fn find(widget: &gtk4::Widget, class: &str) -> Option<gtk4::Widget> {
        if widget.has_css_class(class) {
            return Some(widget.clone());
        }
        let mut child = widget.first_child();
        while let Some(candidate) = child {
            if let Some(found) = find(&candidate, class) {
                return Some(found);
            }
            child = candidate.next_sibling();
        }
        None
    }

    find(parent.as_ref(), class)
        .and_downcast::<T>()
        .unwrap_or_else(|| panic!("no {class} below the widget"))
}

#[test]
#[ignore = "needs a display"]
fn status_island_states() {
    gtk4::init().expect("display");
    crate::register_resources().expect("resources");

    let island = StatusIsland::new();
    let window = gtk4::Window::new();
    window.set_child(Some(&island));
    let imp = island.imp();

    assert!(!island.is_focusable(), "the island itself takes no focus");
    assert!(
        !imp.battery.is_focusable(),
        "an indicator slot takes no focus"
    );
    assert!(
        imp.power_button.is_focusable(),
        "the power button is the one focusable thing on the island"
    );

    assert!(!imp.battery.get_visible());
    let battery = IndicatorSpec {
        label: Some("84%".to_owned()),
        icon: gio::Icon::for_string("battery-level-80-symbolic").ok(),
        ..Default::default()
    };
    island.set_battery(Some(&battery));
    let battery_label: gtk4::Label = child_named(&*imp.battery, "indicator__label");
    assert_eq!(battery_label.text().as_str(), "84%");
    let battery_icon: gtk4::Image = child_named(&*imp.battery, "indicator__icon");
    assert!(
        crate::icons_equal(battery_icon.gicon().as_ref(), battery.icon.as_ref()),
        "the battery indicator shows the spec's icon"
    );
    assert!(imp.battery.get_visible());

    island.set_battery(None);
    assert!(!imp.battery.get_visible(), "None hides the slot");

    let toggled = Rc::new(Cell::new(0));
    island.connect_session_toggled({
        let toggled = Rc::clone(&toggled);
        move |_| toggled.set(toggled.get() + 1)
    });
    assert!(imp.power_button.get_visible());
    imp.power_button.emit_clicked();
    assert_eq!(toggled.get(), 1);
    imp.power_button.emit_clicked();
    assert_eq!(toggled.get(), 2, "each click fires the signal once");

    island.set_session_available(false);
    assert!(!imp.power_button.get_visible());
    island.set_session_available(true);
    assert!(imp.power_button.get_visible());

    window.destroy();
}

fn group(key: &str, app: &str, count: u32) -> ChipGroup {
    ChipGroup {
        key: key.to_owned(),
        app: app.to_owned(),
        icon: None,
        count,
    }
}

fn chip_at(island: &StatusIsland, index: usize) -> Indicator {
    let mut child = island.first_child();
    for _ in 0..index {
        child = child.and_then(|widget| widget.next_sibling());
    }
    child
        .and_downcast::<Indicator>()
        .unwrap_or_else(|| panic!("no chip at {index}"))
}

fn descendant<T: IsA<gtk4::Widget>>(root: &gtk4::Widget) -> Option<T> {
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Ok(found) = widget.clone().downcast::<T>() {
            return Some(found);
        }
        if let Some(found) = descendant(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

fn image_in(chip: &Indicator) -> Option<gtk4::Image> {
    descendant(chip.upcast_ref())
}

fn label_in(chip: &Indicator) -> Option<gtk4::Label> {
    descendant(chip.upcast_ref())
}

#[test]
#[ignore = "needs a display"]
fn status_island_notifications() {
    gtk4::init().expect("display");
    crate::register_resources().expect("resources");

    let chips = StatusIsland::new();
    let window = gtk4::Window::new();
    window.set_child(Some(&chips));

    chips.set_notifications(&[]);
    assert!(chips.imp().chips.borrow().is_empty());

    chips.set_notifications(&[group("slack", "Slack", 3), group("mail", "Mail", 0)]);
    assert_eq!(
        chips.imp().chips.borrow().len(),
        1,
        "a zero-count group is skipped entirely"
    );

    let slack = chip_at(&chips, 0);
    assert!(
        image_in(&slack).is_none_or(|image| image.gicon().is_none()),
        "a None icon reserves nothing on a fresh chip"
    );
    assert_eq!(label_in(&slack).expect("count label").text().as_str(), "3");
    assert_eq!(
        slack.tooltip_text().as_deref(),
        Some("Slack: 3 new notifications"),
        "the tooltip carries the same string as the accessible label"
    );

    chips.set_notifications(&[
        group("slack", "Slack", 3),
        ChipGroup {
            key: "camera".to_owned(),
            app: "Camera".to_owned(),
            icon: Some(gio::ThemedIcon::new("camera-photo-symbolic").upcast()),
            count: 1,
        },
    ]);
    let camera = chip_at(&chips, 1);
    assert!(
        image_in(&camera).is_some_and(|image| image.gicon().is_some()),
        "a Some icon shows the icon slot"
    );

    chips.set_notifications(&[group("mail", "Mail", 1), group("slack", "Slack", 5)]);
    assert_eq!(chips.imp().chips.borrow().len(), 2);
    assert_eq!(
        chip_at(&chips, 1),
        slack,
        "the reordered slack chip reuses its own widget rather than rebuilding it"
    );
    assert_eq!(
        chip_at(&chips, 0).tooltip_text().as_deref(),
        Some("Mail: 1 new notification")
    );

    chips.set_notifications(&[group("slack", "Slack", 5)]);
    assert_eq!(
        chips.imp().chips.borrow().len(),
        1,
        "a dropped key removes its chip"
    );
    assert_eq!(chip_at(&chips, 0), slack, "the surviving chip is unchanged");

    chips.set_notifications(&[]);
    assert_eq!(chips.imp().chips.borrow().len(), 0);

    window.destroy();
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
