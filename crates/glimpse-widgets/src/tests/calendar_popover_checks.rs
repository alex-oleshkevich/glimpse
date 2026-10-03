use super::*;

pub(super) fn check() {
    let popover = CalendarPopover::new();
    let event = |summary: &str| Event {
        id: summary.to_owned(),
        summary: summary.to_owned(),
        when: "09:00 · 1 h".to_owned(),
        ..Default::default()
    };
    let footer = || child_named::<gtk4::Box>(&popover, "popover-shell__footer");

    popover.set_footer(None);
    assert!(
        !child_named::<LegacyRow>(&popover, "calendar-popover__footer").is_visible(),
        "a footer with no label is a row that would do nothing when clicked"
    );
    assert!(
        !footer().is_visible(),
        "an empty footer still costs its own padding and the hairline above it"
    );

    popover.set_footer(Some("Open calendar"));
    assert!(child_named::<LegacyRow>(&popover, "calendar-popover__footer").is_visible());
    assert!(footer().is_visible());

    popover.set_zones(&[]);
    assert!(
        !child_named::<Section>(&popover, "calendar-popover__zones").is_visible(),
        "an empty world clock is a section heading over nothing"
    );

    let few: Vec<Event> = (0..3)
        .map(|index| event(&format!("event {index}")))
        .collect();
    let shown = || popover.imp().events.imp().holders.borrow().len();
    let many: Vec<Event> = (0..9)
        .map(|index| event(&format!("event {index}")))
        .collect();
    popover.set_day("Today", &many);
    assert_eq!(
        shown(),
        4,
        "four events and a row counting the rest; nothing springs open on its own"
    );

    popover.imp().events.imp().more.emit_clicked();
    assert_eq!(
        shown(),
        9,
        "taking the offer shows the rest in place, never in a column beside the list"
    );
    popover.set_day("Today", &many);
    assert_eq!(shown(), 9, "a refresh of the same day keeps it shown");

    popover.set_day("Tuesday", &many);
    assert_eq!(shown(), 4, "another day starts capped again");

    popover.set_day("Tuesday, 29 Sep", &[]);
    assert!(
        !popover.imp().day.get_visible(),
        "a day with nothing on it shows nothing: the grid's missing dot already said so"
    );
    popover.set_day_truncated(true);
    assert!(
        popover.imp().day.get_visible() && popover.imp().truncated.get_visible(),
        "a day the calendar could not send in full says so, since the grid cannot"
    );
    popover.set_day_truncated(false);
    popover.set_day("Today", &few);
    assert!(popover.imp().day.get_visible());

    popover.imp().calendar.show_month(2026, 12);
    let months = Rc::new(RefCell::new(Vec::new()));
    popover.connect_month_shown({
        let months = Rc::clone(&months);
        move |_, year, month| months.borrow_mut().push((year, month))
    });

    popover.imp().calendar.step(1);
    popover.imp().calendar.step(1);
    assert_eq!(
        months.borrow().as_slice(),
        [(2027, 1), (2027, 2)],
        "the panel asks the daemon for the months it is showing, so every step has to reach it"
    );
    assert_eq!(popover.shown_month(), (2027, 2));

    let next = NextEventPopover::new();
    let hero = || next.imp().hero.clone();

    next.set_footer(None);
    assert!(!child_named::<gtk4::Box>(&next, "popover-shell__footer").is_visible());
    next.set_footer(Some("Open calendar"));
    assert!(child_named::<gtk4::Box>(&next, "popover-shell__footer").is_visible());

    next.set_countdown(None);
    assert!(
        !next.imp().countdown.get_visible(),
        "an all-day entry has no minute to count, and an empty readout still costs its slot"
    );
    next.set_countdown(Some(("12", "min")));
    assert!(next.imp().countdown.get_visible());

    next.set_upcoming(&[]);
    assert!(
        !next.imp().upcoming.get_visible(),
        "the popover opens for one event; a caption over an empty list is the empty state \
             this applet does not have"
    );
    next.set_upcoming(&few);
    assert!(next.imp().upcoming.get_visible());

    next.set_heading("Design review", Some("14:00–15:00"));
    assert_eq!(hero().title().as_deref(), Some("Design review"));

    next.set_join(Some((
        "Join Google Meet",
        "meet.google.com/aaa-bbbb-ccc",
        "https://meet.google.com/aaa-bbbb-ccc",
    )));
    assert!(next.imp().join.get_visible());
    next.set_join(None);
    assert!(!next.imp().join.get_visible());

    let opened: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    next.connect_open_event_activated({
        let opened = Rc::clone(&opened);
        move |_, url| opened.borrow_mut().push(url)
    });
    next.set_open_event(Some((
        "Open event",
        "https://calendar.example/event/abc",
        "https://calendar.example/event/abc",
    )));
    assert!(next.imp().open_event.get_visible());
    next.imp().open_event.emit_by_name::<()>("clicked", &[]);
    assert_eq!(*opened.borrow(), ["https://calendar.example/event/abc"]);
    next.set_open_event(None);
    assert!(!next.imp().open_event.get_visible());

    next.set_facts(&[Fact::new("Calendar", "Work")]);
    assert!(next.imp().details.get_visible());
    next.set_facts(&[]);
    assert!(!next.imp().details.get_visible());

    popover.imp().calendar.select(Ymd::new(2027, 2, 3));
    assert_eq!(
        months.borrow().len(),
        2,
        "picking a day inside the month already shown asks the daemon for nothing new"
    );
}
