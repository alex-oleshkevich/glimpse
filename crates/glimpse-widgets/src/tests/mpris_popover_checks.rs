use super::*;

pub(super) fn check() {
    let mpris = MprisPopover::new();
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

    let volumes = Rc::new(RefCell::new(Vec::new()));
    mpris.connect_volume_changed({
        let volumes = volumes.clone();
        move |_, volume| volumes.borrow_mut().push(volume)
    });
    assert!(!mpris.imp().volume.get_visible());
    mpris.set_volume(Some(0.4));
    assert!(mpris.imp().volume.get_visible());
    assert!((mpris.imp().volume.value() - 40.0).abs() < f64::EPSILON);
    mpris.set_volume(Some(f64::NAN));
    assert!(
        (mpris.imp().volume.value() - 40.0).abs() < f64::EPSILON,
        "a NaN from the bus leaves the slider where it was"
    );
    assert!(
        volumes.borrow().is_empty(),
        "following the player is not a request to change it"
    );
    mpris
        .imp()
        .volume
        .emit_by_name::<()>("changed", &[&70.0f64]);
    assert_eq!(volumes.borrow().as_slice(), [0.7]);
    mpris.set_volume(None);
    assert!(!mpris.imp().volume.get_visible());

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
    assert!(!child_named::<gtk4::Box>(&mpris, "popover-shell__footer").is_visible());
    mpris.set_footer(Some("Open Spotify"));
    assert!(child_named::<gtk4::Box>(&mpris, "popover-shell__footer").is_visible());

    assert!(
        mpris.player().scrubber().is_ancestor(&mpris),
        "the primary player is composed whole, not reassembled out of its parts here"
    );
}
