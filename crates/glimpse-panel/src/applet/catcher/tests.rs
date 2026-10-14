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
        catcher.window.keyboard_mode(),
        KeyboardMode::Exclusive,
        "on-demand focus arrives only with a click into the surface, so keys would reach the \
         window behind"
    );
    catcher.set_typing(false);
    assert_eq!(catcher.window.keyboard_mode(), KeyboardMode::None);
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
        capturing_keys(&catcher.window),
        capturing_keys(&plain) + 1,
        "a search entry consumes Escape in the bubble phase, so the popover must see it first"
    );
    plain.destroy();
    catcher.window.destroy();
}
