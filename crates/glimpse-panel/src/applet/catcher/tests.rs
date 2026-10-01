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
