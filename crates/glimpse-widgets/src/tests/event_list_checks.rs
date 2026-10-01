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
    let heads = |list: &EventList| -> Vec<EventRow> {
        list.imp()
            .holders
            .borrow()
            .iter()
            .filter_map(|(_, holder)| holder.head::<EventRow>())
            .collect()
    };
    events.set_events(&[
        event("Team standup", "09:30", Some(blue)),
        event("Design review", "14:00", None),
    ]);
    let event_rows: Vec<Row> = heads(&events).into_iter().map(|row| row.upcast()).collect();
    assert_eq!(event_rows.len(), 2);
    assert_eq!(event_rows[0].title().as_deref(), Some("Team standup"));
    assert!(
        event_rows[1].imp().lead.get_visible(),
        "one event with a color gives every row the same lead column, so the summaries \
             still line up"
    );
    assert_eq!(
        event_rows[0]
            .imp()
            .lead
            .measure(gtk4::Orientation::Horizontal, -1)
            .1,
        (dots::SIZE * 3.0) as i32,
        "an event carries one color, so its lead is one dot wide rather than the three the \
             calendar reserves"
    );
    assert!(
        !event_rows[0].can_target(),
        "an event with nothing behind it and nobody listening does not light up under the \
             pointer: a hover is a promise that clicking does something"
    );
    events.set_activatable(true);
    assert!(event_rows[0].can_target());

    let activated = Rc::new(RefCell::new(Vec::new()));
    events.connect_activated({
        let activated = Rc::clone(&activated);
        move |_, index| activated.borrow_mut().push(index)
    });
    event_rows[1].emit_clicked();
    assert_eq!(*activated.borrow(), [1u32]);
    events.set_activatable(false);

    events.set_events(&[event("Team standup", "09:30", None)]);
    assert!(
        !heads(&events)[0]
            .upcast_ref::<Row>()
            .imp()
            .lead
            .get_visible(),
        "with no color anywhere the lead column goes away rather than sitting empty"
    );
    assert!(
        event_rows[0] == heads(&events)[0].clone().upcast::<Row>(),
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
    let holder = events.imp().holders.borrow()[1].1.clone();
    assert!(
        heads(&events)[1].can_target() && !heads(&events)[0].can_target(),
        "an event with a link or a fact opens; one with neither stays inert"
    );
    heads(&events)[1].emit_clicked();
    assert!(holder.expanded(), "the event row opens its own card");
    let card = holder.details::<gtk4::Box>().expect("a card");
    events.set_events(&[event("Team standup", "10:00", None), meeting.clone()]);
    assert!(
        holder.details::<gtk4::Box>().as_ref() == Some(&card),
        "a refresh that leaves the card's content alone keeps the card, so a press on it lands"
    );
    let links = Rc::new(RefCell::new(Vec::new()));
    events.connect_link_activated({
        let links = Rc::clone(&links);
        move |_, url| links.borrow_mut().push(url)
    });
    assert!(
        card.first_child().and_downcast::<FactList>().is_some(),
        "the facts lead the card, before its links"
    );
    children_of::<Row>(&card)
        .first()
        .expect("the join row")
        .emit_clicked();
    assert_eq!(*links.borrow(), ["https://meet.google.com/abc"]);

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
            .map(|row| row.upcast_ref::<Row>().title().unwrap_or_default())
            .collect()
    };
    assert_eq!(
        titles(&events),
        ["Three", "Four", "Five"],
        "what is over folds away and the cap counts only what is left"
    );
    assert_eq!(events.imp().earlier.title().as_deref(), Some("2 earlier"));
    assert_eq!(
        events.imp().more.title().as_deref(),
        Some("1 more event"),
        "one hidden event is not `1 more events`"
    );
    events.imp().earlier.emit_clicked();
    assert_eq!(titles(&events).len(), 5, "the fold opens in place");
    assert!(!events.imp().earlier.get_visible());
    assert!(heads(&events)[0].has_css_class("event-list__row--past"));
    events.imp().more.emit_clicked();
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
            .upcast_ref::<Row>()
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
            .upcast_ref::<Row>()
            .title()
            .unwrap_or_default()
            .chars()
            .count(),
        TEXT_MAX_CHARS,
        "a row inside a hidden section still reports what it was given: a widget's own \
             `visible` flag is not the same question as whether an ancestor is showing"
    );
}
