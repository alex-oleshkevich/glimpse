use super::*;

pub(super) fn check() {
    let event = |summary: &str, when: &str, color: Option<gdk::RGBA>| Event {
        id: summary.to_owned(),
        summary: summary.to_owned(),
        when: when.to_owned(),
        color,
        ..Default::default()
    };
    let blue = gdk::RGBA::new(0.2, 0.5, 0.9, 1.0);

    let events = EventList::new();
    let heads = |list: &EventList| -> Vec<Row> {
        list.imp()
            .holders
            .borrow()
            .iter()
            .map(|(_, holder)| match holder.downcast_ref::<ExpanderRow>() {
                Some(expander) => expander.imp().head.get().upcast(),
                None => holder.clone().downcast().expect("a row"),
            })
            .collect()
    };
    events.set_events(&[
        event("Team standup", "09:30", Some(blue)),
        event("Design review", "14:00", None),
    ]);
    let event_rows = heads(&events);
    assert_eq!(event_rows.len(), 2);
    assert_eq!(event_rows[0].title().as_deref(), Some("Team standup"));
    assert_eq!(event_rows[0].value().as_deref(), Some("09:30"));
    assert!(
        event_rows[1].imp().start.get_visible(),
        "one event with a color gives every row the same start column, so the summaries \
             still line up"
    );
    assert_eq!(
        event_rows[0]
            .imp()
            .start
            .measure(gtk4::Orientation::Horizontal, -1)
            .1,
        (dots::SIZE * 3.0) as i32,
        "an event carries one color, so its start is one dot wide"
    );
    assert!(
        !event_rows[0].is::<ActionRow>(),
        "an event with nothing behind it is a static row: a hover is a promise that \
             clicking does something"
    );

    events.set_events(&[event("Team standup", "09:30", None)]);
    assert!(
        !heads(&events)[0].imp().start.get_visible(),
        "with no color anywhere the start column goes away rather than sitting empty"
    );
    assert!(
        event_rows[0] == heads(&events)[0],
        "an event keeps its row across a refresh rather than rebuilding it"
    );
    assert!(
        event_rows[1].parent().is_none(),
        "a shorter list unparents the rows it no longer has events for"
    );

    let meeting = Event {
        links: vec![EventLink {
            title: "Join Google Meet".to_owned(),
            url: "https://meet.google.com/abc".to_owned(),
        }],
        facts: vec![Fact::new("Calendar", "Work")],
        ..event("Design review", "14:00", None)
    };
    events.set_events(&[event("Team standup", "09:30", None), meeting.clone()]);
    let holder = events.imp().holders.borrow()[1]
        .1
        .clone()
        .downcast::<ExpanderRow>()
        .expect("an event with a link or a fact opens");
    holder.imp().head.emit_by_name::<()>("activated", &[]);
    assert!(holder.open(), "the event row opens its own drawer");
    let card = holder.details();
    let first = card.first_child();
    events.set_events(&[event("Team standup", "10:00", None), meeting.clone()]);
    assert!(
        card.first_child() == first,
        "a refresh that leaves the drawer's content alone keeps it, so a press on it lands"
    );
    let links = Rc::new(RefCell::new(Vec::new()));
    events.connect_link_activated({
        let links = Rc::clone(&links);
        move |_, url| links.borrow_mut().push(url)
    });
    assert!(
        card.first_child().and_downcast::<FactList>().is_some(),
        "the facts lead the drawer, before its links"
    );
    children_of::<ActionRow>(&card)
        .first()
        .expect("the join row")
        .emit_by_name::<()>("activated", &[]);
    assert_eq!(*links.borrow(), ["https://meet.google.com/abc"]);

    events.set_events(&[event("Design review", "14:00", None)]);
    events.set_events(std::slice::from_ref(&meeting));
    let reopened = events.imp().holders.borrow()[0]
        .1
        .clone()
        .downcast::<ExpanderRow>()
        .expect("an expander again");
    assert!(
        reopened.details().first_child().is_some(),
        "an event that loses its drawer and gets it back is filled again"
    );

    let past = |summary: &str| Event {
        past: true,
        ..event(summary, "over", None)
    };
    events.set_events(&[
        past("One"),
        past("Two"),
        event("Three", "11:00", None),
        event("Four", "12:00", None),
        event("Five", "13:00", None),
        event("Six", "14:00", None),
    ]);
    events.set_max_rows(3);
    let titles = |list: &EventList| -> Vec<String> {
        heads(list)
            .iter()
            .map(|row| row.title().unwrap_or_default())
            .collect()
    };
    assert_eq!(
        titles(&events),
        ["Three", "Four", "Five"],
        "what is over folds away and the cap counts only what is left"
    );
    assert_eq!(
        events.imp().earlier.upcast_ref::<Row>().title().as_deref(),
        Some("2 earlier")
    );
    assert_eq!(
        events.imp().more.upcast_ref::<Row>().title().as_deref(),
        Some("1 more event"),
        "one hidden event is not `1 more events`"
    );
    events.imp().earlier.emit_by_name::<()>("activated", &[]);
    assert_eq!(titles(&events).len(), 5, "the fold opens in place");
    assert!(!events.imp().earlier.get_visible());
    assert!(
        heads(&events)[0].inactive(),
        "what is over is inactive, and still opens"
    );
    events.imp().more.emit_by_name::<()>("activated", &[]);
    assert_eq!(titles(&events).len(), 6);
    assert!(!events.overflows());
    events.fold();
    assert_eq!(titles(&events).len(), 3, "folding restores both caps");

    events.set_max_rows(0);
    events.set_events(&[event("One", "09:30", None)]);
    assert!(
        !events.imp().earlier.get_visible() && !events.imp().more.get_visible(),
        "a list that fits carries neither count"
    );

    events.set_events(&[event(&"ё".repeat(TEXT_MAX_CHARS * 2), "09:30", None)]);
    assert_eq!(
        heads(&events)[0]
            .title()
            .unwrap_or_default()
            .chars()
            .count(),
        TEXT_MAX_CHARS,
        "a calendar summary is another application's string: capped without slicing a \
             multi-byte character"
    );

    let buried = Section::new();
    buried.set_content(Some(&events));
    buried.set_empty(true);
    assert_eq!(
        heads(&events)[0]
            .title()
            .unwrap_or_default()
            .chars()
            .count(),
        TEXT_MAX_CHARS,
        "a row inside a hidden section still reports what it was given: a widget's own \
             `visible` flag is not the same question as whether an ancestor is showing"
    );
}
