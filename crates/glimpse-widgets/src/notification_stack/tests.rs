use super::{imp::interpolate, summary};

#[test]
fn the_chip_counts_what_is_behind_the_front_card_as_well_as_the_front_card() {
    assert_eq!(summary(2), "2 notifications");
    assert_eq!(summary(1), "1 notification");
}

#[test]
fn stack_geometry_interpolates_between_collapsed_and_expanded_positions() {
    assert_eq!(interpolate(80, 240, 0.0), 80);
    assert_eq!(interpolate(80, 240, 0.5), 160);
    assert_eq!(interpolate(80, 240, 1.0), 240);
}
