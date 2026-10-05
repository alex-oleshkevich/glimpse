use super::*;

pub(super) fn check() {
    let mpris = MprisPopover::new();
    let column = mpris.player().parent().unwrap();
    assert_eq!(
        mpris.imp().scroller.parent().as_ref(),
        Some(mpris.imp().card.upcast_ref())
    );
    assert!(column.is_ancestor(&mpris.imp().scroller.get()));
    assert_eq!(
        column.first_child().as_ref(),
        Some(mpris.player().upcast_ref())
    );
    assert!(!mpris.player().get_visible());
    assert_eq!(
        column.last_child().as_ref(),
        Some(mpris.imp().footer.upcast_ref())
    );
    let seen = Rc::new(RefCell::new(Vec::new()));
    mpris.connect_raise_requested({
        let seen = seen.clone();
        move |_, key| seen.borrow_mut().push(("raise", key))
    });
    mpris.connect_toggle_requested({
        let seen = seen.clone();
        move |_, key| seen.borrow_mut().push(("toggle", key))
    });

    mpris.set_others(None);
    assert!(!mpris.imp().others.get_visible());
    mpris.set_others(Some(&[]));
    assert!(
        !mpris.imp().others.get_visible(),
        "one player running shows no section at all rather than one saying so"
    );
    mpris.set_others(Some(&[
        Player {
            key: "firefox".to_owned(),
            name: "Firefox".to_owned(),
            icon_name: "firefox".to_owned(),
            title: "Texas Sun".to_owned(),
            artist: "Khruangbin".to_owned(),
            playing: false,
        },
        Player {
            key: "mpv".to_owned(),
            name: "mpv".to_owned(),
            icon_name: "mpv".to_owned(),
            title: "Pisces".to_owned(),
            artist: "Jinjer".to_owned(),
            playing: true,
        },
    ]));
    assert!(mpris.imp().others.get_visible());

    let rows = mpris.imp().list.imp().rows.borrow().clone();
    rows[1].emit_by_name::<()>("clicked", &[]);
    rows[0].emit_by_name::<()>("toggled", &[]);
    assert_eq!(
        *seen.borrow(),
        [
            ("raise", "mpv".to_owned()),
            ("toggle", "firefox".to_owned())
        ],
        "a row reports the key of whatever player currently occupies it, through the popover"
    );

    mpris.set_footer(None);
    assert!(!mpris.imp().footer.is_visible());
    mpris.set_footer(Some("Open Spotify"));
    assert!(mpris.imp().footer.is_visible());

    assert!(
        mpris.player().scrubber().is_ancestor(&mpris),
        "the primary player is composed whole, not reassembled out of its parts here"
    );

    let crowded = MprisPopover::new();
    let players: Vec<_> = (0..40)
        .map(|index| Player {
            key: index.to_string(),
            name: format!("Player {index}"),
            ..Default::default()
        })
        .collect();
    crowded.set_others(Some(&players));
    crowded.set_footer(Some("Open player"));
    let window = gtk4::Window::new();
    window.set_default_size(500, 200);
    window.set_child(Some(&crowded));
    window.present();
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let scroller = &crowded.imp().scroller;
    while (scroller.max_content_height() < 0 || scroller.height() == 0)
        && std::time::Instant::now() < deadline
    {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let cap = scroller.max_content_height();
    assert!(cap > 0, "the mapped MPRIS popover has a monitor height cap");
    let (_, natural, _, _) = scroller.measure(gtk4::Orientation::Vertical, 500);
    assert!(
        natural <= cap,
        "additional players cannot exceed the height cap"
    );
    let scroll = scroller.vadjustment();
    assert!(scroll.upper() > scroll.page_size());
    scroll.set_value(scroll.upper() - scroll.page_size());
    assert!(scroll.value() > 0.0, "the lower controls remain scrollable");
    window.close();
}
