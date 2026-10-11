use super::*;

/// Separate from `widgets()` for the same reason as `clipboard_widgets`: one failure earlier in
/// that function must not hide every assertion written after it.
#[test]
#[ignore = "needs a display"]
fn places_popover_widgets() {
    if gtk4::init().is_err() {
        return;
    }
    register_resources().expect("resources");
    let _styles = Styles::install(adw::ColorScheme::Default);

    let popover = PlacesPopover::new();
    let imp = popover.imp();

    assert!(
        !imp.places.get_visible()
            && !imp.bookmarks.get_visible()
            && !imp.network.get_visible()
            && !imp.trash.get_visible(),
        "an untouched popover shows no section"
    );

    let entry = |id: &str, title: &str| PlacesEntry {
        id: id.to_owned(),
        title: title.to_owned(),
        ..Default::default()
    };

    let emptied = Rc::new(Cell::new(0));
    popover.connect_empty_trash({
        let emptied = Rc::clone(&emptied);
        move |_| emptied.set(emptied.get() + 1)
    });
    popover.set_trash(Some(PlacesTrash { items: 3 }));
    imp.trash_empty.activate();
    assert_eq!(emptied.get(), 0, "the first press only asks");
    assert_eq!(
        imp.trash_empty.upcast_ref::<Row>().title().as_deref(),
        Some("Delete 3 items for good?")
    );
    imp.trash_item.set_open(true);
    imp.trash_item.set_open(false);
    assert_eq!(
        imp.trash_empty.upcast_ref::<Row>().title().as_deref(),
        Some("Empty the trash"),
        "closing the card takes the question back"
    );
    imp.trash_empty.activate();
    imp.trash_empty.activate();
    assert_eq!(emptied.get(), 1, "the second press empties");
    popover.set_trash(Some(PlacesTrash { items: 0 }));
    assert!(
        !imp.trash_empty.get_visible(),
        "an empty trash offers nothing to empty"
    );

    popover.set_places(&[]);
    popover.set_bookmarks(&[]);
    popover.set_network(&[]);
    popover.set_trash(None);
    assert!(
        !imp.places.get_visible()
            && !imp.bookmarks.get_visible()
            && !imp.network.get_visible()
            && !imp.trash.get_visible(),
        "an empty section and one given nothing at all are the same: no section renders"
    );

    // Places and Bookmarks: hidden when empty, reused by key rather than position.
    popover.set_places(&[entry("home", "Home"), entry("docs", "Documents")]);
    assert!(imp.places.get_visible());
    let home_widget = imp.places_rows.first_child();
    popover.set_places(&[entry("docs", "Documents"), entry("home", "Home")]);
    assert_eq!(
        imp.places_rows.last_child(),
        home_widget,
        "a position reuses its widget rather than rebuilding it"
    );

    let activated = Rc::new(RefCell::new(None));
    popover.connect_activated({
        let activated = Rc::clone(&activated);
        move |_, id| {
            activated.replace(Some(id.to_owned()));
        }
    });
    home_widget
        .and_downcast::<ActionRow>()
        .expect("a row")
        .activate();
    assert_eq!(
        activated.borrow().as_deref(),
        Some("home"),
        "identity follows the key a row was built for, not the position it sits at now"
    );

    popover.set_bookmarks(&[entry("dl", "Downloads")]);
    assert!(imp.bookmarks.get_visible());
    popover.set_bookmarks(&[]);
    assert!(!imp.bookmarks.get_visible());

    popover.set_network(&[entry("files", "files")]);
    assert!(imp.network.get_visible());
    popover.set_network(&[]);
    assert!(
        !imp.network.get_visible(),
        "an empty Network hides like every other section rather than standing empty"
    );

    // Trash: a known count of zero is "Empty", not absence either.
    popover.set_trash(Some(PlacesTrash { items: 0 }));
    assert!(imp.trash.get_visible());
    assert_eq!(imp.trash_item.value().as_deref(), Some("Empty"));
    assert_eq!(
        imp.trash_item.icon_name().as_deref(),
        Some("user-trash-symbolic")
    );
    popover.set_trash(Some(PlacesTrash { items: 363 }));
    assert_eq!(imp.trash_item.value().as_deref(), Some("363 items"));
    assert_eq!(
        imp.trash_item.icon_name().as_deref(),
        Some("user-trash-full-symbolic")
    );
    popover.set_trash(None);
    assert!(!imp.trash.get_visible());

    // A hostile multibyte label is capped by characters, not bytes, and the card holds its
    // width. Both measurements keep the same sections visible, so the comparison isolates
    // label length rather than a section appearing for the first time.
    let width = |popover: &PlacesPopover| popover.measure(gtk4::Orientation::Horizontal, -1).1;
    popover.set_places(&[entry("a", "Home")]);
    popover.set_bookmarks(&[entry("b", "Downloads")]);
    popover.set_network(&[]);
    popover.set_trash(None);
    let floor = width(&popover);

    let hostile = "\u{1f642}".repeat(400);
    popover.set_bookmarks(&[entry("h", &hostile)]);
    assert_eq!(
        width(&popover),
        floor,
        "a 400-character bookmark label must not widen the card"
    );

    let more = Rc::new(Cell::new(false));
    popover.connect_more({
        let more = Rc::clone(&more);
        move |_| more.set(true)
    });
    popover.set_overflow(Some("3 more"));
    imp.bookmarks_more.activate();
    assert!(more.get());

}
