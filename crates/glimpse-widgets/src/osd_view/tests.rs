use super::*;
use gtk4::{prelude::*, subclass::prelude::*};
use std::cell::Cell;
use std::rc::Rc;

#[test]
#[ignore = "needs a display"]
fn renders_optional_content_and_overamplification() {
    gtk4::init().expect("display");
    crate::register_resources().expect("resources");
    let view = OsdView::new();
    let imp = view.imp();
    assert!(!imp.icon.get_visible());
    assert!(!imp.label.get_visible());
    assert!(!imp.meter.get_visible());
    assert!(!imp.percent.get_visible());
    assert_eq!(view.value(), 0.0);
    assert!(!view.has_value());

    view.set_icon_name("audio-volume-high-symbolic");
    view.set_label("Spotify");
    view.set_value(0.45);
    view.set_has_value(true);
    assert!(imp.icon.get_visible());
    assert!(imp.label.get_visible());
    assert!(imp.meter.get_visible());
    assert!(!imp.percent.get_visible());
    assert!(!view.show_percent());
    assert_eq!(imp.meter.fraction(), 0.45);
    assert!(!imp.root.has_css_class("osd-view--over"));

    view.set_show_percent(true);
    assert!(imp.percent.get_visible());
    assert_eq!(imp.percent.text(), "45%");

    view.set_value(1.3);
    assert_eq!(imp.meter.fraction(), 1.0);
    assert_eq!(imp.percent.text(), "130%");
    assert!(imp.root.has_css_class("osd-view--over"));
    view.set_has_value(false);
    assert!(!imp.meter.get_visible());
    assert!(!imp.percent.get_visible());
    assert!(!imp.root.has_css_class("osd-view--over"));
    view.set_has_value(true);
    assert!(imp.root.has_css_class("osd-view--over"));

    view.set_value(0.0);
    assert_eq!(imp.meter.fraction(), 0.0);
    assert_eq!(imp.percent.text(), "0%");
    assert!(!imp.root.has_css_class("osd-view--over"));
    view.set_icon_name("");
    view.set_label("");
    assert!(!imp.icon.get_visible());
    assert!(!imp.label.get_visible());
}

#[test]
#[ignore = "needs a display"]
fn caps_multibyte_labels_and_skips_unchanged_writes() {
    gtk4::init().expect("display");
    crate::register_resources().expect("resources");
    let view = OsdView::new();
    view.set_icon_name("input-keyboard-symbolic");
    view.set_label("界".repeat(300));
    view.set_value(0.3);
    view.set_has_value(true);
    assert_eq!(view.label().chars().count(), LABEL_MAX_CHARS);
    assert_eq!(view.imp().label.text(), "界".repeat(LABEL_MAX_CHARS));

    let notifications = Rc::new(Cell::new(0));
    for object in [
        view.clone().upcast::<glib::Object>(),
        view.imp().icon.get().upcast::<glib::Object>(),
        view.imp().label.get().upcast::<glib::Object>(),
        view.imp().meter.get().upcast::<glib::Object>(),
        view.imp().percent.get().upcast::<glib::Object>(),
        view.imp().root.get().upcast::<glib::Object>(),
    ] {
        let notifications = notifications.clone();
        object.connect_notify_local(None, move |_, _| {
            notifications.set(notifications.get() + 1);
        });
    }
    view.set_icon_name("input-keyboard-symbolic");
    view.set_label("界".repeat(300));
    view.set_value(0.3);
    view.set_has_value(true);
    assert_eq!(notifications.get(), 0);
    view.set_label("English (US)");
    assert!(notifications.get() > 0);
}

#[test]
#[ignore = "needs a display"]
fn preserves_cleaned_label_ellipsis() {
    gtk4::init().expect("display");
    crate::register_resources().expect("resources");
    let view = OsdView::new();
    let label = format!("{}…", "界".repeat(120));
    view.set_label(label.clone());
    assert_eq!(view.label(), label);
    assert_eq!(view.imp().label.text(), label);
}
