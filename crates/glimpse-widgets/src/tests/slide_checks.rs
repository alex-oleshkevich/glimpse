use super::*;
use std::time::{Duration, Instant};

pub(super) fn check(styles: &Styles) {
    let settings = gtk4::Settings::default().expect("GTK settings");
    let animations = settings.property::<bool>("gtk-enable-animations");
    settings.set_property("gtk-enable-animations", true);

    unmapped_slide_finishes_at_once();
    superseded_slide_drops_its_callback();
    zero_speed_slides_in_one_step(styles);
    hidden_slide_still_paints();

    settings.set_property("gtk-enable-animations", animations);
}

fn unmapped_slide_finishes_at_once() {
    let slide = Slide::new();
    let child = gtk4::Label::new(Some("bar"));
    slide.set_child(Some(&child));
    let done = Rc::new(Cell::new(false));
    slide.reveal(false, {
        let done = done.clone();
        move || done.set(true)
    });
    assert!(done.get());
    assert!(!slide.shown());
    assert!(!child.can_target());
    slide.jump(true);
    assert!(slide.shown());
    assert!(child.can_target());
}

fn superseded_slide_drops_its_callback() {
    let slide = Slide::new();
    slide.set_child(Some(&gtk4::Label::new(Some("bar"))));
    let window = gtk4::Window::new();
    window.set_child(Some(&slide));
    window.present();
    wait_until(|| slide.is_mapped() && slide.height() > 0);

    let first = Rc::new(Cell::new(false));
    let second = Rc::new(Cell::new(false));
    slide.reveal(false, {
        let first = first.clone();
        move || first.set(true)
    });
    slide.reveal(true, {
        let second = second.clone();
        move || second.set(true)
    });
    wait_until(|| second.get());
    assert!(!first.get());
    assert!(slide.shown());
    window.close();
}

fn zero_speed_slides_in_one_step(styles: &Styles) {
    let slide = Slide::new();
    slide.set_child(Some(&gtk4::Label::new(Some("bar"))));
    let window = gtk4::Window::new();
    window.set_child(Some(&slide));
    window.present();
    wait_until(|| slide.is_mapped() && slide.height() > 0);

    styles.set_animation_speed(0.0);
    let done = Rc::new(Cell::new(false));
    slide.reveal(false, {
        let done = done.clone();
        move || done.set(true)
    });
    wait_until(|| done.get());
    assert!(!slide.shown());
    styles.set_animation_speed(1.0);
    window.close();
}

fn hidden_slide_still_paints() {
    let slide = Slide::new();
    slide.set_child(Some(&gtk4::Label::new(Some("bar"))));
    let window = gtk4::Window::new();
    window.set_child(Some(&slide));
    window.present();
    wait_until(|| slide.is_mapped() && slide.height() > 0);

    slide.jump(false);
    let ticks = Rc::new(Cell::new(0));
    let tick = slide.add_tick_callback({
        let ticks = ticks.clone();
        move |_, _| {
            ticks.set(ticks.get() + 1);
            glib::ControlFlow::Continue
        }
    });
    wait_until(|| ticks.get() > 1);
    tick.remove();

    let snapshot = gtk4::Snapshot::new();
    gtk4::WidgetPaintable::new(Some(&slide)).snapshot(
        &snapshot,
        f64::from(slide.width()),
        f64::from(slide.height()),
    );
    assert!(
        snapshot.to_node().is_some(),
        "a hidden slide paints a node, or a window holding only it never commits its surface"
    );
    window.close();
}

fn wait_until(ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let context = glib::MainContext::default();
    while !ready() {
        assert!(Instant::now() < deadline, "slide timed out");
        context.iteration(false);
    }
}
