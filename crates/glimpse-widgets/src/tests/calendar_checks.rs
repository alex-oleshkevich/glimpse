use super::*;

pub(super) fn check() {
    let calendar = Calendar::new();
    calendar.set_today(Ymd::new(2026, 9, 23));
    calendar.show_month(2026, 9);
    let today_button = child_named::<gtk4::Button>(&calendar, "calendar__today");
    assert!(
        !today_button.is_visible(),
        "Today is meaningless while today's month is shown and nothing else is selected"
    );

    calendar.select(Ymd::new(2026, 9, 17));
    assert!(
        today_button.is_visible(),
        "the way back has to be offered whenever the selection is not today, even on today's \
             own month"
    );
    today_button.emit_clicked();
    assert_eq!(calendar.selected(), Some(Ymd::new(2026, 9, 23)));
    assert!(
        !today_button.is_visible(),
        "taking the way back leaves nothing to go back to"
    );

    calendar.step(1);
    assert_eq!(calendar.shown(), (2026, 10));
    assert!(today_button.is_visible());
    calendar.step(-4);
    assert_eq!(
        calendar.shown(),
        (2026, 6),
        "stepping crosses months, not weeks"
    );

    let chosen = Rc::new(RefCell::new(Vec::new()));
    calendar.connect_day_selected({
        let chosen = Rc::clone(&chosen);
        move |_, date| chosen.borrow_mut().push(date)
    });
    calendar.select(Ymd::new(2026, 6, 4));
    assert_eq!(calendar.selected(), Some(Ymd::new(2026, 6, 4)));
    assert_eq!(
        chosen.borrow().as_slice(),
        &[Ymd::new(2026, 6, 4)],
        "selecting a day reports it once, with the day it was given"
    );

    let scope_button = child_named::<gtk4::Button>(&calendar, "calendar__scope");
    let title = child_named::<gtk4::Label>(&calendar, "calendar__title");
    scope_button.emit_clicked();
    assert_eq!(
        title.text().as_str(),
        "2026",
        "the title is the zoom control: clicking it widens the scope to the year"
    );
    let months = all_named(&calendar, "calendar__month");
    assert_eq!(months.len(), 12);
    assert!(
        months[5].has_css_class("calendar__cell--selected"),
        "the year view marks the month it was opened from, so widening the scope does not \
             lose where you were"
    );
    assert!(
        !months[0].has_css_class("calendar__cell--selected"),
        "and marks only that one"
    );

    scope_button.emit_clicked();
    assert_ne!(title.text().as_str(), "2026");

    let repeats = Rc::new(Cell::new(0));
    calendar.connect_day_selected({
        let repeats = Rc::clone(&repeats);
        move |calendar, date| {
            repeats.set(repeats.get() + 1);
            calendar.select(date);
        }
    });
    calendar.select(Ymd::new(2026, 6, 11));
    assert_eq!(
        repeats.get(),
        1,
        "selecting the day that is already selected reports nothing, so a handler that \
             reselects in response does not drive the signal round for ever"
    );

    calendar.clear_selection();
    assert_eq!(calendar.selected(), None);

    let red = gtk4::gdk::RGBA::new(1.0, 0.0, 0.0, 1.0);
    calendar.set_events(&[(Ymd::new(2026, 6, 4), vec![red; 5])]);
    assert_eq!(
        calendar.events(Ymd::new(2026, 6, 4)).len(),
        3,
        "three dots is a cap, not a count: a fourth event adds nothing and shifts nothing"
    );
}
