use super::*;

pub(super) fn check() {
    let scrubber = Scrubber::new();
    let track = child_named::<gtk4::Scale>(&scrubber, "scrubber__track");
    let times = all_named(&scrubber, "scrubber__time");
    let elapsed = times[0].clone().downcast::<gtk4::Label>().expect("elapsed");
    let remaining = times[1]
        .clone()
        .downcast::<gtk4::Label>()
        .expect("remaining");

    assert!(
        !scrubber.seekable() && !track.is_sensitive(),
        "a scrubber starts unseekable, matching a template whose scale is already insensitive; \
             a widget that disagrees with its own blueprint at birth is wrong before anyone \
             touches it"
    );
    scrubber.set_seekable(true);
    assert!(track.is_sensitive());

    scrubber.set_duration(405.0);
    scrubber.set_position(167.0);
    assert_eq!(elapsed.text(), "2:47");
    assert_eq!(
        remaining.text(),
        "\u{2212}3:58",
        "the right-hand figure counts down, with a real minus sign rather than a hyphen"
    );
    assert_eq!(scrubber.position(), 167.0);

    assert_eq!(
        (
            track.adjustment().step_increment(),
            track.adjustment().page_increment()
        ),
        (5.0, 30.0),
        "an arrow key moves five seconds and Page Up thirty. These are set from Rust because \
             blueprint-compiler's adjustment rule rejects an adjustment carrying anything besides \
             lower, upper and value, so nothing in the template guards them"
    );

    scrubber.imp().held.set(Some(167.0));
    scrubber.set_position(300.0);
    assert_eq!(
        scrubber.position(),
        167.0,
        "a player reporting its position once a second loses to a drag in progress, or the \
             slider is pulled out from under the pointer every time one lands"
    );
    scrubber.imp().held.set(None);
    scrubber.set_position(300.0);
    assert_eq!(scrubber.position(), 300.0);
    scrubber.set_position(167.0);

    let seeks = Rc::new(RefCell::new(Vec::new()));
    scrubber.connect_seek({
        let seeks = Rc::clone(&seeks);
        move |_, seconds| seeks.borrow_mut().push(seconds)
    });
    scrubber.emit_by_name::<()>("seek", &[&12.0f64]);
    assert_eq!(*seeks.borrow(), [12.0f64]);

    scrubber.set_duration(0.0);
    assert!(
        !track.get_visible(),
        "a stream with no length has nothing to scrub, so the track goes rather than sitting \
             there full or empty and lying about it"
    );
    assert!(
        !remaining.get_visible(),
        "and nothing remains of a length nobody knows"
    );
}
