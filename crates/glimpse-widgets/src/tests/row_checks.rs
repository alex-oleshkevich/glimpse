use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;

use super::*;
use crate::{
    ActionRow, MeterRow, Row, RowTone, RowTypography, TEXT_MAX_CHARS, ToggleControl, ToggleRow,
};

pub(super) fn checks() {
    tone_and_type();
    static_row();
    slots();
    hidden_slots();
    text_caps();
    compare_before_write();
    action();
    toggle();
    meter();
    roles();
    gap();
    inactive_input();
}

fn gap() {
    let row = Row::new();
    row.set_icon_name(Some("folder-symbolic"));
    row.set_title(Some("Folder"));
    let content = &row.imp().content;
    assert_eq!(content.spacing(), 0);
    let (_, natural, _, _) = content.measure(gtk4::Orientation::Horizontal, -1);
    let mut sum = 0;
    let mut child = content.first_child();
    while let Some(widget) = child {
        if widget.get_visible() {
            sum += widget.measure(gtk4::Orientation::Horizontal, -1).1;
        }
        child = widget.next_sibling();
    }
    assert_eq!(natural - sum, 10);
}

fn tone_and_type() {
    let row = Row::new();
    assert!(!row.has_css_class("row"));
    assert_eq!(row.css_name().as_str(), "listrow");
    for (tone, class) in [
        (RowTone::Warning, "row--warning"),
        (RowTone::Error, "row--error"),
        (RowTone::Destructive, "row--destructive"),
    ] {
        row.set_tone(tone);
        for candidate in ["row--warning", "row--error", "row--destructive"] {
            assert_eq!(row.has_css_class(candidate), candidate == class);
        }
    }
    let title = child_named::<gtk4::Label>(&row, "row__title");
    assert!(row.has_css_class("row--destructive"));
    assert!(title.has_css_class("row__title"));
    row.set_tone(RowTone::Normal);
    assert!(!row.has_css_class("row--warning"));
    assert!(!row.has_css_class("row--error"));
    assert!(!row.has_css_class("row--destructive"));

    row.set_typography(RowTypography::Numeric);
    assert!(row.has_css_class("row--numeric"));
    assert!(!row.has_css_class("row--monospace"));
    row.set_typography(RowTypography::Monospace);
    assert!(row.has_css_class("row--monospace"));
    assert!(!row.has_css_class("row--numeric"));
    row.set_typography(RowTypography::Default);
    assert!(!row.has_css_class("row--numeric"));
    assert!(!row.has_css_class("row--monospace"));
}

fn static_row() {
    let row = Row::new();
    assert!(row.can_target());
    assert!(!row.is_focusable());
    assert!(!row.has_css_class("row--interactive"));
    row.set_tooltip(Some("Where the pointer rests"));
    assert!(row.has_tooltip());
}

fn slots() {
    let row = Row::new();
    row.set_value(Some("42"));
    let value = child_named::<gtk4::Label>(&row, "row__value");
    assert!(value.get_visible());
    let mark = gtk4::Image::from_icon_name("go-next-symbolic");
    row.set_end(&mark);
    assert!(
        value.get_visible(),
        "a trailing widget sits beside the value"
    );
    let end = row.imp().end();
    assert!(end.get_visible());
    row.set_busy(true);
    let spinner = child_named::<gtk4::Spinner>(&row, "row__spinner");
    assert!(spinner.get_visible());
    assert!(spinner.is_spinning());
    assert!(!value.get_visible());
    assert!(!end.get_visible());
    row.set_busy(false);
    assert!(!spinner.get_visible());
    assert!(end.get_visible());
    assert!(value.get_visible());
}

fn hidden_slots() {
    let parent = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let row = Row::new();
    parent.append(&row);
    parent.set_visible(false);
    let value = child_named::<gtk4::Label>(&row, "row__value");
    let spinner = child_named::<gtk4::Spinner>(&row, "row__spinner");
    row.set_busy(true);
    assert!(spinner.get_visible());
    row.set_busy(false);
    assert!(!spinner.get_visible());
    row.set_value(Some("42"));
    assert!(value.get_visible());
    row.set_value(None::<&str>);
    assert!(!value.get_visible());
}

fn text_caps() {
    let row = Row::new();
    let long = "я".repeat(10_000);
    row.set_title(Some(long.as_str()));
    row.set_subtitle(Some(long.as_str()));
    row.set_value(Some(long.as_str()));
    row.set_tooltip(Some(long.as_str()));
    for text in [row.title(), row.subtitle(), row.value(), row.tooltip()] {
        let text = text.expect("capped text");
        assert!(text.chars().count() <= TEXT_MAX_CHARS);
        assert_eq!(text.chars().count(), TEXT_MAX_CHARS);
        assert!(text.chars().all(|ch| ch == 'я'));
    }
}

fn compare_before_write() {
    let row = Row::new();
    let title = child_named::<gtk4::Label>(&row, "row__title");
    let notifies = Rc::new(Cell::new(0u32));
    title.connect_notify_local(Some("label"), {
        let notifies = notifies.clone();
        move |_, _| notifies.set(notifies.get() + 1)
    });
    row.set_title(Some("Same"));
    row.set_title(Some("Same"));
    assert_eq!(notifies.get(), 1);
}

fn action() {
    let row = ActionRow::new();
    let window = gtk4::Window::new();
    window.set_child(Some(&row));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_eq!(row.css_name().as_str(), "listrow");
    assert!(row.is_focusable());
    assert!(row.has_css_class("row--interactive"));
    assert!(!row.upcast_ref::<Row>().imp().chevron.get().get_visible());
    let hits = Rc::new(Cell::new(0u32));
    row.connect_activated({
        let hits = hits.clone();
        move |_| hits.set(hits.get() + 1)
    });
    key(&row, gtk4::gdk::Key::Return);
    key(&row, gtk4::gdk::Key::Return);
    assert_eq!(hits.get(), 1);
    key_release(&row, gtk4::gdk::Key::Return);
    key(&row, gtk4::gdk::Key::Return);
    key(&row, gtk4::gdk::Key::KP_Enter);
    key(&row, gtk4::gdk::Key::space);
    assert_eq!(hits.get(), 4);
    click(
        &row,
        f64::from(row.width()) / 2.0,
        f64::from(row.height()) / 2.0,
    );
    assert_eq!(hits.get(), 5);
    window.close();
}

fn toggle() {
    let row = ToggleRow::new();
    let window = gtk4::Window::new();
    window.set_child(Some(&row));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_eq!(row.css_name().as_str(), "listrow");
    assert!(!row.imp().knob.get().can_target());
    assert!(!row.imp().knob.get().can_focus());
    let end = row.upcast_ref::<Row>().imp().end();
    assert_eq!(
        row.imp().knob.get().parent().as_ref(),
        Some(end.upcast_ref())
    );
    let hits = Rc::new(RefCell::new(Vec::new()));
    row.connect_toggled({
        let hits = hits.clone();
        move |_, active| hits.borrow_mut().push(active)
    });
    key(&row, gtk4::gdk::Key::Return);
    key(&row, gtk4::gdk::Key::Return);
    assert_eq!(hits.borrow().as_slice(), &[true]);
    key_release(&row, gtk4::gdk::Key::Return);
    key(&row, gtk4::gdk::Key::KP_Enter);
    key(&row, gtk4::gdk::Key::space);
    click(
        &row,
        f64::from(row.width()) - 2.0,
        f64::from(row.height()) / 2.0,
    );
    assert_eq!(hits.borrow().as_slice(), &[true, false, true, false]);
    assert!(!row.active());

    hits.borrow_mut().clear();
    row.upcast_ref::<Row>().set_inactive(true);
    assert!(!row.is_sensitive());
    key(&row, gtk4::gdk::Key::space);
    assert!(!row.active());
    assert!(hits.borrow().is_empty());

    row.upcast_ref::<Row>().set_inactive(false);
    assert!(row.has_css_class("row--interactive"));
    row.set_active(true);
    assert!(row.active());
    assert!(hits.borrow().is_empty());

    row.set_control(ToggleControl::Check);
    let check = child_named::<gtk4::Image>(&row, "row__check");
    assert!(check.get_visible());
    assert!(row.has_css_class("row--on"));
    assert!(!row.imp().knob.get().get_visible());
    row.set_active(false);
    assert!(!check.get_visible());
    assert!(!row.has_css_class("row--on"));
    window.close();
}

fn meter() {
    let row = MeterRow::new();
    assert_eq!(row.css_name().as_str(), "listrow");
    let bar = child_named::<gtk4::ProgressBar>(&row, "meter-row__bar");
    assert_eq!(row.level(), -1.0);
    assert!(!bar.get_visible());
    assert!(!row.is_focusable());
    row.set_level(f64::NAN);
    assert_eq!(row.level(), -1.0);
    assert!(!bar.get_visible());
    row.set_level(-0.5);
    assert_eq!(row.level(), -1.0);
    assert!(!bar.get_visible());
    row.set_level(1.5);
    assert_eq!(row.level(), 1.0);
    assert!(bar.get_visible());
    assert!((bar.fraction() - 1.0).abs() < f64::EPSILON);
    row.set_level(f64::NAN);
    assert_eq!(row.level(), -1.0);
    assert!(!bar.get_visible());
}

fn roles() {
    let row = Row::new();
    row.set_title(Some("Static"));
    row.set_subtitle(Some("Description"));
    assert_eq!(row.accessible_role(), gtk4::AccessibleRole::ListItem);
    assert!(gtk4::test_accessible_has_property(
        &row,
        gtk4::AccessibleProperty::Label
    ));
    assert!(gtk4::test_accessible_has_property(
        &row,
        gtk4::AccessibleProperty::Description
    ));
    assert_eq!(
        ActionRow::new().accessible_role(),
        gtk4::AccessibleRole::Button
    );
    let toggle = ToggleRow::new();
    toggle.set_property("title", Some("Enabled"));
    assert_eq!(toggle.accessible_role(), gtk4::AccessibleRole::Switch);
    toggle.set_control(ToggleControl::Check);
    assert_eq!(toggle.accessible_role(), gtk4::AccessibleRole::Checkbox);
    key(&toggle, gtk4::gdk::Key::space);
    assert!(toggle.active());
    assert!(gtk4::test_accessible_has_state(
        &toggle,
        gtk4::AccessibleState::Checked
    ));
    assert!(gtk4::test_accessible_has_property(
        &toggle,
        gtk4::AccessibleProperty::Label
    ));
    assert_eq!(
        MeterRow::new().accessible_role(),
        gtk4::AccessibleRole::ProgressBar
    );

    let box_ = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let action = ActionRow::new();
    box_.append(&row);
    box_.append(&action);
    box_.append(&toggle);
    let window = gtk4::Window::new();
    window.set_child(Some(&box_));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert_eq!(
        gtk4::prelude::GtkWindowExt::focus(&window).as_ref(),
        Some(action.upcast_ref())
    );
    assert!(box_.child_focus(gtk4::DirectionType::TabForward));
    assert_eq!(
        gtk4::prelude::GtkWindowExt::focus(&window).as_ref(),
        Some(toggle.upcast_ref())
    );
    assert_ne!(
        gtk4::prelude::GtkWindowExt::focus(&window).as_ref(),
        Some(toggle.imp().knob.get().upcast_ref())
    );
    window.close();
}

fn key_controller(row: &impl IsA<gtk4::Widget>) -> gtk4::EventControllerKey {
    let controllers = row.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .find_map(|controller| controller.downcast::<gtk4::EventControllerKey>().ok())
        .expect("row key controller")
}

fn key(row: &impl IsA<gtk4::Widget>, key: gtk4::gdk::Key) {
    key_controller(row).emit_by_name::<bool>(
        "key-pressed",
        &[&key, &0u32, &gtk4::gdk::ModifierType::empty()],
    );
}

fn key_release(row: &impl IsA<gtk4::Widget>, key: gtk4::gdk::Key) {
    key_controller(row).emit_by_name::<()>(
        "key-released",
        &[&key, &0u32, &gtk4::gdk::ModifierType::empty()],
    );
}

pub(super) fn click(row: &impl IsA<gtk4::Widget>, x: f64, y: f64) {
    let controllers = row.observe_controllers();
    let click = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .find_map(|controller| controller.downcast::<gtk4::GestureClick>().ok())
        .expect("row click gesture");
    click.emit_by_name::<()>("released", &[&1i32, &x, &y]);
}

fn inactive_input() {
    let action = ActionRow::new();
    let toggle = ToggleRow::new();
    toggle.set_active(true);
    let hits = Rc::new(Cell::new(0));
    action.connect_activated({
        let hits = hits.clone();
        move |_| hits.set(hits.get() + 1)
    });
    toggle.connect_toggled({
        let hits = hits.clone();
        move |_, _| hits.set(hits.get() + 1)
    });
    let rows = [
        Row::new(),
        action.clone().upcast::<Row>(),
        toggle.clone().upcast::<Row>(),
    ];
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    for row in &rows {
        row.set_title(Some("Device"));
        row.set_subtitle(Some("Device out of range"));
        body.append(row);
    }
    let window = gtk4::Window::new();
    window.set_child(Some(&body));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    for row in &rows {
        let interactive = row.has_css_class("row--interactive");
        row.set_inactive(true);
        assert!(!row.is_sensitive() && row.has_css_class("row--inactive"));
        assert!(row.imp().subtitle.get_visible());
        if row.is::<ActionRow>() || row.is::<ToggleRow>() {
            click(
                row,
                f64::from(row.width()) / 2.0,
                f64::from(row.height()) / 2.0,
            );
            for pressed in [
                gtk4::gdk::Key::space,
                gtk4::gdk::Key::Return,
                gtk4::gdk::Key::KP_Enter,
            ] {
                key(row, pressed);
                key_release(row, pressed);
            }
        }
        row.activate();
        assert_eq!(hits.get(), 0);
        assert!(toggle.active());
        row.set_inactive(false);
        assert!(row.is_sensitive() && !row.has_css_class("row--inactive"));
        assert_eq!(row.has_css_class("row--interactive"), interactive);
    }
    action.activate();
    toggle.activate();
    assert_eq!(hits.get(), 2);
    assert!(!toggle.active());
    window.close();
}
