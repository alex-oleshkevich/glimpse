use super::*;

#[test]
#[ignore = "needs a display"]
fn clipboard_widgets() {
    gtk4::init().expect("display");
    register_resources().expect("resources");
    let _styles = Styles::install(adw::ColorScheme::Default);

    let clipboard = ClipboardPopover::new();
    clipboard.set_actions(ClipActions {
        pin: "Pin".to_owned(),
        unpin: "Unpin".to_owned(),
        forget: "Forget".to_owned(),
    });
    let clip = |id: u64, pinned: bool| Clip {
        id,
        title: "Марта🙂 a rather long clipboard entry that keeps going".to_owned(),
        icon: "text-x-generic-symbolic".to_owned(),
        pinned,
        ..Default::default()
    };
    clipboard.set_pinned(&[clip(1, true)]);
    clipboard.set_recent(&[clip(2, false), clip(3, false)]);

    let rows = |list: &ClipboardList| children_of::<ExpanderRow>(list);
    assert_eq!(rows(&clipboard.imp().recent).len(), 2);
    assert!(
        clipboard.imp().pinned_section.get_visible(),
        "a pinned entry gives the section something to show"
    );

    let width = |popover: &ClipboardPopover| popover.measure(gtk4::Orientation::Horizontal, -1).1;
    let floor = width(&clipboard);

    let dimmed = |widget: &gtk4::Widget| {
        std::iter::successors(Some(widget.clone()), |widget| widget.parent())
            .any(|widget| widget.has_css_class("receded"))
    };
    let opened = rows(&clipboard.imp().recent)[0].clone();
    opened.set_open(true);
    assert!(
        dimmed(clipboard.imp().shell.imp().header.upcast_ref()),
        "the header is read against the open panel and must recede with everything else"
    );
    assert!(
        dimmed(clipboard.imp().clear.upcast_ref()),
        "the clear row recedes while a card asks a question"
    );
    assert!(
        opened.open() && !dimmed(opened.upcast_ref()),
        "the row that was opened is the one thing that must not recede"
    );
    assert!(
        dimmed(rows(&clipboard.imp().recent)[1].upcast_ref()),
        "its neighbour recedes"
    );
    assert_eq!(
        width(&clipboard),
        floor,
        "opening a detail must not resize the card under the pointer that opened it"
    );

    opened.set_open(false);
    assert!(!dimmed(clipboard.imp().shell.imp().header.upcast_ref()));

    opened.set_open(true);
    let pin = |row: &ExpanderRow| {
        all_named(&row.details(), "clipboard-list__pin")
            .pop()
            .and_downcast::<ActionRow>()
            .expect("a pin row")
    };
    let before = pin(&opened);
    clipboard.set_recent(&[clip(2, true), clip(3, false)]);
    assert_eq!(
        pin(&opened),
        before,
        "a republish must leave the open panel's buttons alone, even one that changes them"
    );
    assert_eq!(
        before.upcast_ref::<Row>().title().as_deref(),
        Some("Unpin"),
        "and the pin row's wording follows the entry"
    );

    let cleared = Rc::new(Cell::new(false));
    clipboard.connect_cleared({
        let cleared = Rc::clone(&cleared);
        move |_| cleared.set(true)
    });
    clipboard.imp().clear.emit_by_name::<()>("activated", &[]);
    assert!(cleared.get());

    let restored = Rc::new(Cell::new(None));
    clipboard.connect_restored({
        let restored = Rc::clone(&restored);
        move |_, id| restored.set(Some(id))
    });
    let second = rows(&clipboard.imp().recent)[1].clone();
    second.set_open(true);
    children_of::<ActionRow>(&second.details())[0].emit_by_name::<()>("activated", &[]);
    assert_eq!(
        restored.get(),
        Some(3),
        "a row emits the entry it currently holds, not the one it was built for"
    );

    clipboard.set_recent(&[]);
    assert!(
        !clipboard.imp().recent_section.get_visible(),
        "an empty Recent hides rather than contradicting the Pinned list above it"
    );

    let acted = Rc::new(RefCell::new(Vec::new()));
    clipboard.connect_acted({
        let acted = Rc::clone(&acted);
        move |_, id, key| acted.borrow_mut().push((id, key))
    });
    let long = "a long line of text that the row cannot hold whole";
    let texture: gdk::Texture = gdk::MemoryTexture::new(
        32,
        18,
        gdk::MemoryFormat::R8g8b8,
        &glib::Bytes::from(&vec![128u8; 32 * 18 * 3]),
        32 * 3,
    )
    .upcast();
    clipboard.set_recent(&[
        Clip {
            id: 10,
            title: "#1e88e5".to_owned(),
            swatch: Some(gdk::RGBA::new(0.1, 0.5, 0.9, 1.0)),
            actions: vec![ClipAction {
                key: "rgb".to_owned(),
                title: "Copy as RGB".to_owned(),
                value: "rgb(30 136 229)".to_owned(),
                ..Default::default()
            }],
            facts: vec![Fact::new("Size", "7 B")],
            ..Default::default()
        },
        Clip {
            id: 11,
            title: "Image · 1 kB".to_owned(),
            image: Some(texture),
            ..Default::default()
        },
        Clip {
            id: 12,
            title: long.to_owned(),
            excerpt: long.to_owned(),
            ..Default::default()
        },
    ]);
    let cells = children_of::<gtk4::Widget>(&*clipboard.imp().recent);
    let color = cells[0]
        .clone()
        .downcast::<ExpanderRow>()
        .expect("a color row");
    assert_eq!(color.icon_name(), None, "a color is marked by its swatch");
    color.set_open(true);
    let card = color.details();
    assert!(card.first_child().and_downcast::<Swatch>().is_some());
    children_of::<ActionRow>(&card)
        .into_iter()
        .find(|row| row.upcast_ref::<Row>().title().as_deref() == Some("Copy as RGB"))
        .expect("the type's own action")
        .emit_by_name::<()>("activated", &[]);
    assert_eq!(*acted.borrow(), [(10, "rgb".to_owned())]);

    let image = cells[1]
        .downcast_ref::<Expandable>()
        .expect("an image card");
    let tile = image
        .head::<gtk4::Overlay>()
        .expect("an image is a tile, not a row");
    tile.child()
        .and_downcast::<gtk4::Button>()
        .expect("the tile's body is a button")
        .emit_by_name::<()>("clicked", &[]);
    assert_eq!(
        restored.get(),
        Some(11),
        "a click on the image copies it back"
    );
    assert!(!image.expanded(), "and does not open the card");
    let badge = children_of::<gtk4::Button>(&tile)
        .into_iter()
        .find(|button| button.has_css_class("clip-tile__badge"))
        .expect("a badge");
    badge.emit_by_name::<()>("clicked", &[]);
    assert!(image.expanded(), "the badge opens the card");

    let before = width(&clipboard);
    cells[2]
        .downcast_ref::<ExpanderRow>()
        .expect("a text row")
        .set_open(true);
    assert_eq!(
        width(&clipboard),
        before,
        "a long excerpt wraps inside the card rather than widening the popover"
    );

    let more = Rc::new(Cell::new(0));
    clipboard.connect_more({
        let more = Rc::clone(&more);
        move |_| more.set(more.get() + 1)
    });
    clipboard.set_overflow(Some("3 more"));
    clipboard.set_recent(&[]);
    assert!(
        clipboard.imp().recent_section.get_visible(),
        "entries still behind the overflow row keep the section up"
    );
    clipboard.imp().more.emit_by_name::<()>("activated", &[]);
    assert_eq!(more.get(), 1);

    let searched = Rc::new(RefCell::new(String::new()));
    clipboard.connect_searched({
        let searched = Rc::clone(&searched);
        move |_, query| {
            searched.replace(query);
        }
    });
    clipboard.set_searchable(true);
    assert!(clipboard.imp().search.get_visible());
    clipboard.imp().search.set_text("review");
    clipboard
        .imp()
        .search
        .emit_by_name::<()>("search-changed", &[]);
    assert_eq!(*searched.borrow(), "review");
}

#[test]
#[ignore = "needs a display"]
fn a_clip_that_gains_its_image_becomes_a_tile() {
    gtk4::init().expect("display");
    register_resources().expect("resources");
    let _styles = Styles::install(adw::ColorScheme::Default);

    let list = ClipboardList::new();
    let bare = Clip {
        id: 7,
        title: "Image · 1 kB".to_owned(),
        ..Default::default()
    };
    list.set_clips(std::slice::from_ref(&bare));
    assert_eq!(children_of::<ExpanderRow>(&list).len(), 1);

    let texture: gdk::Texture = gdk::MemoryTexture::new(
        4,
        4,
        gdk::MemoryFormat::R8g8b8,
        &glib::Bytes::from(&[128u8; 48]),
        12,
    )
    .upcast();
    list.set_clips(&[Clip {
        image: Some(texture),
        ..bare
    }]);
    assert!(children_of::<ExpanderRow>(&list).is_empty());
    assert_eq!(children_of::<Expandable>(&list).len(), 1);
}
