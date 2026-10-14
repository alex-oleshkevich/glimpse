use super::placement;

const ARROW: i32 = 20;

#[test]
fn a_popover_centers_on_its_item() {
    let (start, offset) = placement(500, 400, 1920, ARROW);
    assert_eq!((start, offset), (300, 190));
    assert_eq!(start + offset + ARROW / 2, 500);
}

#[test]
fn the_arrow_stays_on_the_item_when_the_body_cannot_center_on_it() {
    let (start, offset) = placement(28, 418, 1913, 18);
    assert_eq!(start + offset + 18 / 2, 28);
    assert!(
        offset >= 18,
        "the arrow rode the rounded corner at {offset}"
    );
}

#[test]
fn a_popover_that_cannot_center_still_keeps_a_gutter_from_the_edge() {
    let (start, offset) = placement(100, 418, 1913, 18);
    assert_eq!(start, 18, "the body sat flush against the output edge");
    assert_eq!(start + offset + 18 / 2, 100);
}

#[test]
fn a_popover_never_leaves_the_output() {
    for center in [0, 28, 500, 1900, 1920] {
        let (start, _) = placement(center, 400, 1920, ARROW);
        assert!(
            start >= 0 && start + 400 <= 1920,
            "center {center} put the body at {start}"
        );
    }
}

#[test]
fn the_arrow_never_sits_on_the_bodys_rounded_corner() {
    for center in [0, 5, 28, 500, 1900, 1920] {
        let (_, offset) = placement(center, 400, 1920, ARROW);
        assert!(
            (ARROW..=400 - ARROW).contains(&offset),
            "center {center} put the arrow at {offset}"
        );
    }
}

#[test]
fn an_unmeasured_popover_asks_for_nothing_out_of_range() {
    assert_eq!(placement(120, 0, 0, 0), (0, 0));
}

pub(crate) fn every_arrow_paints_over_the_body_on_its_panel_side() {
    use glimpse_config::Position;
    use gtk4::prelude::*;

    for (side, arrow, body) in [
        (Position::Top, (0, 0), (0, 1)),
        (Position::Bottom, (0, 1), (0, 0)),
        (Position::Left, (0, 0), (1, 0)),
        (Position::Right, (1, 0), (0, 0)),
    ] {
        let catcher = super::Catcher::new(None, side);
        let cell = |widget: &gtk4::Widget| {
            let (column, row, _, _) = catcher.slot.query_child(widget);
            (column, row)
        };
        assert_eq!(
            catcher.slot.last_child().as_ref(),
            Some(catcher.arrow.upcast_ref::<gtk4::Widget>()),
            "{side:?}: the arrow is drawn after the body, so the body's shadow never covers it"
        );
        assert_eq!(cell(catcher.arrow.upcast_ref()), arrow, "{side:?}");
        assert_eq!(cell(catcher.body.upcast_ref()), body, "{side:?}");
        catcher.window.destroy();
    }
}

pub(crate) fn a_typing_popover_takes_the_keyboard_without_a_click() {
    use gtk4::prelude::*;
    use gtk4_layer_shell::{KeyboardMode, LayerShell};

    let catcher = super::Catcher::new(None, glimpse_config::Position::Bottom);
    catcher.set_typing(true);
    assert_eq!(
        catcher.popup.keyboard_mode(),
        KeyboardMode::Exclusive,
        "on-demand focus arrives only with a click into the surface, so keys would reach the \
         window behind"
    );
    assert_eq!(
        catcher.window.keyboard_mode(),
        KeyboardMode::None,
        "the entry is on the popover surface, not the catcher"
    );
    catcher.set_typing(false);
    assert_eq!(catcher.popup.keyboard_mode(), KeyboardMode::None);
    let capturing_keys = |window: &gtk4::Window| {
        window
            .observe_controllers()
            .iter::<gtk4::glib::Object>()
            .filter_map(Result::ok)
            .filter_map(|object| object.downcast::<gtk4::EventControllerKey>().ok())
            .filter(|controller| controller.propagation_phase() == gtk4::PropagationPhase::Capture)
            .count()
    };
    let plain = gtk4::Window::new();
    assert_eq!(
        capturing_keys(&catcher.popup),
        capturing_keys(&plain) + 1,
        "a search entry consumes Escape in the bubble phase, so the popover must see it first"
    );
    plain.destroy();
    catcher.window.destroy();
}

pub(crate) fn only_the_body_sized_surface_is_blurred() {
    use glimpse_config::Position;
    use gtk4::prelude::*;
    use gtk4_layer_shell::{Edge, LayerShell};

    let monitor = gtk4::gdk::Display::default()
        .and_then(|display| display.monitors().item(0))
        .and_downcast::<gtk4::gdk::Monitor>()
        .expect("a monitor");
    let probe = gtk4::Window::new();
    glimpse_widgets::blur::apply(&probe, true);
    for side in [
        Position::Top,
        Position::Bottom,
        Position::Left,
        Position::Right,
    ] {
        let catcher = super::Catcher::new(Some(&monitor), side);
        catcher.reconfigure(&monitor, side, true);
        assert!(
            !catcher.window.has_css_class("blurred"),
            "{side:?}: a compositor blurs the whole surface carrying a blur region"
        );
        assert_eq!(
            catcher.popup.has_css_class("blurred"),
            probe.has_css_class("blurred"),
            "{side:?}"
        );
        assert_eq!(
            catcher.body.root().as_ref(),
            Some(catcher.popup.upcast_ref::<gtk4::Root>()),
            "{side:?}"
        );
        let spans = |window: &gtk4::Window, a, b| window.is_anchor(a) && window.is_anchor(b);
        assert!(
            spans(&catcher.window, Edge::Left, Edge::Right)
                && spans(&catcher.window, Edge::Top, Edge::Bottom),
            "{side:?}: the catcher covers the output"
        );
        assert!(
            !spans(&catcher.popup, Edge::Left, Edge::Right)
                && !spans(&catcher.popup, Edge::Top, Edge::Bottom),
            "{side:?}: the blurred surface is sized to the body, never stretched over the output"
        );
    }
    probe.destroy();
}

pub(crate) fn the_popover_surface_follows_its_body() {
    use glimpse_config::Position;
    use gtk4::prelude::*;
    use gtk4_layer_shell::{Edge, LayerShell};
    use std::time::{Duration, Instant};

    let until = |done: &dyn Fn() -> bool| {
        let context = gtk4::glib::MainContext::default();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !done() && Instant::now() < deadline {
            while context.iteration(false) {}
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let monitor = gtk4::gdk::Display::default()
        .and_then(|display| display.monitors().item(0))
        .and_downcast::<gtk4::gdk::Monitor>()
        .expect("a monitor");
    let catcher = super::Catcher::new(Some(&monitor), Position::Top);
    let child = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    child.set_size_request(300, 200);
    catcher.open(&child, 0, || {});
    until(&|| catcher.room() > 0 && catcher.popup.height() > 0);
    let center = catcher.room() / 2;
    catcher.place(center);

    let placed = |width: i32, height: i32| {
        until(&|| catcher.body.width() == width && catcher.body.height() == height);
        until(&|| {
            catcher.popup.margin(Edge::Left) + catcher.slot.margin_start() + width / 2 == center
                && catcher.popup.width() == width + 2 * super::GUTTER
                && catcher.popup.height() == catcher.arrow.height() + height + super::GUTTER
        });
        assert_eq!(
            catcher.popup.margin(Edge::Left) + catcher.slot.margin_start() + width / 2,
            center,
            "a {width}-wide body is centred on its item"
        );
        assert_eq!(
            catcher.popup.width(),
            width + 2 * super::GUTTER,
            "the surface is the body and its shadow, not the output"
        );
        assert_eq!(
            catcher.popup.height(),
            catcher.arrow.height() + height + super::GUTTER,
            "a {height}-tall body resizes its surface"
        );
    };
    placed(300, 200);
    child.set_size_request(500, 400);
    placed(500, 400);
    child.set_size_request(300, 100);
    placed(300, 100);
    catcher.close();
    until(&|| !catcher.popup.is_visible());
    assert!(!catcher.window.is_visible());
}
