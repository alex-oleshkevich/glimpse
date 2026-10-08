use super::*;

#[test]
fn a_pointer_maps_to_the_buffer_pixel_under_it_at_a_fractional_scale() {
    let logical = (2304.0, 1440.0);
    let buffer = (2880, 1800);

    assert_eq!(pixel_at(0.0, 0.0, logical, buffer), (0, 0));
    assert_eq!(pixel_at(100.0, 100.0, logical, buffer), (125, 125));
    assert_eq!(pixel_at(100.7, 0.5, logical, buffer), (125, 0));
    assert_eq!(pixel_at(100.9, 0.9, logical, buffer), (126, 1));
}

#[test]
fn a_pointer_on_or_past_the_far_edge_reads_the_last_pixel() {
    let logical = (2304.0, 1440.0);
    let buffer = (2880, 1800);

    assert_eq!(pixel_at(2304.0, 1440.0, logical, buffer), (2879, 1799));
    assert_eq!(pixel_at(-4.0, 99999.0, logical, buffer), (0, 1799));
}

#[test]
fn scrolling_zooms_in_steps_and_stays_between_one_and_the_maximum() {
    assert_eq!(zoomed(8, -1.0, 30), 10);
    assert_eq!(zoomed(8, 1.0, 30), 6);
    assert_eq!(zoomed(1, -1.0, 30), 2);
    assert_eq!(zoomed(2, 1.0, 30), 1);
    assert_eq!(zoomed(28, -1.0, 30), 30);
    assert_eq!(zoomed(30, -1.0, 30), 30);
    assert_eq!(zoomed(1, 1.0, 30), 1);
    assert_eq!(zoomed(4, 0.0, 30), 4);
    assert_eq!(zoomed(8, -1.0, 5), 5);
}

#[test]
fn the_whole_range_is_a_dozen_notches_either_way() {
    let mut zoom = 1;
    let mut notches = 0;
    while zoom < 30 {
        zoom = zoomed(zoom, -1.0, 30);
        notches += 1;
    }
    assert!(notches <= 16, "took {notches} notches");
}
