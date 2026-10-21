use super::*;

use crate::entry_row::ENTRY_MAX_CHARS;

pub(super) fn check() {
    let row = EntryRow::new();
    let entry = child_named::<gtk4::Entry>(&row, "entry-row__entry");
    let title = child_named::<gtk4::Label>(&row, "entry-row__title");
    let end = child_named::<gtk4::Box>(&row, "entry-row__end");
    assert_eq!(row.css_name().as_str(), "listrow");
    assert!(!title.get_visible());
    assert!(!end.get_visible());

    let notified = Rc::new(Cell::new(0));
    row.connect_notify_local(Some("title"), {
        let notified = Rc::clone(&notified);
        move |_, _| notified.set(notified.get() + 1)
    });
    row.set_title(Some("New alarm"));
    row.set_title(Some("New alarm"));
    assert_eq!(notified.get(), 1);
    assert!(title.get_visible());
    assert!(gtk4::test_accessible_has_property(
        &entry,
        gtk4::AccessibleProperty::Label
    ));
    row.set_title(Some("ж".repeat(TEXT_MAX_CHARS + 20).as_str()));
    assert_eq!(
        row.title().map(|text| text.chars().count()),
        Some(TEXT_MAX_CHARS)
    );
    row.set_title(None::<&str>);
    assert!(!title.get_visible());

    row.set_placeholder(Some("10m tea"));
    assert_eq!(entry.placeholder_text().as_deref(), Some("10m tea"));

    let changed = Rc::new(Cell::new(0));
    let activated = Rc::new(Cell::new(0));
    row.connect_changed({
        let changed = Rc::clone(&changed);
        move |_| changed.set(changed.get() + 1)
    });
    row.connect_activated({
        let activated = Rc::clone(&activated);
        move |_| activated.set(activated.get() + 1)
    });
    row.set_text("hello");
    assert_eq!(changed.get(), 0);
    entry.set_position(3);
    row.set_text("hello world");
    assert_eq!(entry.position(), 3);
    row.set_text("hi");
    assert_eq!(entry.position(), 2);
    assert_eq!(changed.get(), 0);
    let mut position = 2;
    entry.insert_text("!", &mut position);
    assert_eq!(changed.get(), 1);
    assert_eq!(row.text(), "hi!");
    entry.emit_activate();
    assert_eq!(activated.get(), 1);

    row.set_text("é".repeat(ENTRY_MAX_CHARS + 10));
    assert_eq!(row.text().chars().count(), ENTRY_MAX_CHARS);

    row.set_tone(RowTone::Warning);
    assert!(row.has_css_class("row--warning"));
    row.set_tone(RowTone::Error);
    assert!(!row.has_css_class("row--warning"));
    assert!(row.has_css_class("row--error"));
    row.set_tone(RowTone::Normal);
    assert!(!row.has_css_class("row--error"));

    let button = gtk4::Button::new();
    row.set_end(&button);
    assert_eq!(button.parent().as_ref(), Some(end.upcast_ref()));
    assert!(end.get_visible());
    row.set_inactive(true);
    assert!(row.has_css_class("row--inactive"));
    assert!(!entry.is_sensitive());
    assert!(!button.is_sensitive());
    row.set_inactive(false);
    assert!(entry.is_sensitive());
    row.clear_end();
    assert!(button.parent().is_none());
    assert!(!end.get_visible());

    row.set_tooltip(Some("Type a time"));
    assert_eq!(row.tooltip_text().as_deref(), Some("Type a time"));
}
