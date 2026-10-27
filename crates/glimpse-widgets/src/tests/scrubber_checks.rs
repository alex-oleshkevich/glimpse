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
    assert!(accessible_set_value(&track, 12.0));
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

fn accessible_set_value(track: &gtk4::Scale, seconds: f64) -> bool {
    use glib::translate::ToGlibPtr;

    let interface = track
        .interface::<gtk4::AccessibleRange>()
        .expect("scale accessible range");
    let set_value = interface
        .as_ref()
        .set_current_value
        .expect("scale accessible value setter");
    let range = track.upcast_ref::<gtk4::AccessibleRange>();
    unsafe { set_value(range.to_glib_none().0, seconds) != glib::ffi::GFALSE }
}

#[test]
#[ignore = "needs a display"]
fn accessible_seeking_distinguishes_user_commands_from_backend_updates() {
    gtk4::init().expect("display");
    register_resources().expect("resources");
    let scrubber = Scrubber::new();
    let track = child_named::<gtk4::Scale>(&scrubber, "scrubber__track");
    let seeks = Rc::new(RefCell::new(Vec::new()));
    scrubber.connect_seek({
        let seeks = Rc::clone(&seeks);
        move |_, seconds| seeks.borrow_mut().push(seconds)
    });
    scrubber.set_seekable(true);
    scrubber.set_duration(180.0);
    scrubber.set_position(30.0);
    assert!(seeks.borrow().is_empty());

    assert!(accessible_set_value(&track, 75.0));
    assert_eq!(scrubber.position(), 75.0);
    assert_eq!(*seeks.borrow(), [75.0]);
    seeks.borrow_mut().clear();

    scrubber.set_position(90.0);
    scrubber.set_duration(60.0);
    assert_eq!(scrubber.position(), 60.0);
    assert!(seeks.borrow().is_empty());

    scrubber.set_duration(180.0);
    scrubber.set_position(30.0);
    track.emit_by_name::<()>("move-slider", &[&gtk4::ScrollType::StepForward]);
    assert_eq!(scrubber.position(), 35.0);
    assert_eq!(*seeks.borrow(), [35.0]);
    seeks.borrow_mut().clear();

    scrubber.set_seekable(false);
    let _ = accessible_set_value(&track, 90.0);
    track.set_value(100.0);
    assert!(seeks.borrow().is_empty());
}

#[test]
#[ignore = "needs a display"]
fn nested_backend_observations_leave_subsequent_accessible_intent_enabled() {
    gtk4::init().expect("display");
    register_resources().expect("resources");
    let scrubber = Scrubber::new();
    let track = child_named::<gtk4::Scale>(&scrubber, "scrubber__track");
    scrubber.set_seekable(true);
    scrubber.set_duration(180.0);
    let seeks = Rc::new(RefCell::new(Vec::new()));
    scrubber.connect_seek({
        let seeks = Rc::clone(&seeks);
        move |_, seconds| seeks.borrow_mut().push(seconds)
    });
    let nested = Rc::new(std::cell::Cell::new(false));
    let handler = track.connect_value_changed(glib::clone!(
        #[weak]
        scrubber,
        move |_| {
            if !nested.replace(true) {
                scrubber.set_duration(120.0);
                scrubber.set_position(45.0);
            }
        }
    ));
    scrubber.set_position(90.0);
    assert_eq!(scrubber.position(), 45.0);
    assert!(seeks.borrow().is_empty());
    track.disconnect(handler);

    assert!(accessible_set_value(&track, 75.0));
    assert_eq!(*seeks.borrow(), [75.0]);
}

#[test]
#[ignore = "needs a display"]
fn held_scrubbing_commits_only_changed_user_intent_on_release() {
    gtk4::init().expect("display");
    register_resources().expect("resources");
    let scrubber = Scrubber::new();
    let track = child_named::<gtk4::Scale>(&scrubber, "scrubber__track");
    let seeks = Rc::new(RefCell::new(Vec::new()));
    scrubber.connect_seek({
        let seeks = Rc::clone(&seeks);
        move |_, seconds| seeks.borrow_mut().push(seconds)
    });
    scrubber.set_seekable(true);
    scrubber.set_duration(180.0);
    scrubber.set_position(100.0);
    scrubber.imp().held.set(Some(100.0));
    scrubber.set_duration(60.0);
    assert_eq!(scrubber.position(), 60.0);
    scrubber.imp().release();
    assert!(seeks.borrow().is_empty());

    scrubber.set_duration(180.0);
    scrubber.set_position(30.0);
    scrubber.imp().held.set(Some(30.0));
    assert!(accessible_set_value(&track, 75.0));
    assert!(seeks.borrow().is_empty());
    scrubber.set_duration(60.0);
    assert_eq!(scrubber.position(), 60.0);
    scrubber.imp().release();
    assert_eq!(*seeks.borrow(), [60.0]);
    seeks.borrow_mut().clear();

    scrubber.set_duration(180.0);
    scrubber.set_position(30.0);
    scrubber.imp().held.set(Some(30.0));
    assert!(accessible_set_value(&track, 75.0));
    assert!(accessible_set_value(&track, 30.0));
    scrubber.imp().release();
    assert!(seeks.borrow().is_empty());

    scrubber.imp().held.set(Some(30.0));
    assert!(accessible_set_value(&track, 75.0));
    scrubber.set_seekable(false);
    scrubber.imp().release();
    assert!(seeks.borrow().is_empty());
    scrubber.set_seekable(true);
    assert!(accessible_set_value(&track, 90.0));
    assert_eq!(*seeks.borrow(), [90.0]);
}
