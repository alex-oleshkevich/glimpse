use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::calendar_popover::CalendarPopover;

static CHILDREN_LEFT: AtomicUsize = AtomicUsize::new(0);

fn init() {
    gtk4::init().expect("display");
    adw::init().expect("adwaita");
    crate::register_resources().expect("resources");
}

#[test]
#[ignore = "needs a display"]
fn a_dropped_calendar_is_finalized() {
    glib::log_set_writer_func(|level, fields| {
        let leftover = fields.iter().any(|field| {
            field.key() == "MESSAGE"
                && field
                    .value_str()
                    .is_some_and(|message| message.contains("still has children left"))
        });
        if leftover {
            CHILDREN_LEFT.fetch_add(1, Ordering::Relaxed);
        }
        glib::log_writer_default(level, fields)
    });
    init();

    let calendar = Calendar::new();
    let weak = calendar.downgrade();
    drop(calendar);

    assert!(weak.upgrade().is_none());
    assert_eq!(CHILDREN_LEFT.load(Ordering::Relaxed), 0);
}

#[test]
#[ignore = "needs a display"]
fn a_calendar_in_a_dropped_popover_is_finalized() {
    init();

    let popover = CalendarPopover::new();
    let weak = popover.imp().calendar.downgrade();
    drop(popover);

    assert!(weak.upgrade().is_none());
}
