use super::*;

#[test]
fn the_meridiem_is_looked_for_in_a_rendered_time_rather_than_in_a_pattern() {
    let shown = locale_afternoon();

    assert!(
        shown.contains("30"),
        "a locale's own time must render the minute, got {shown:?}"
    );
}

#[test]
fn the_measurement_item_is_the_category_shifted_into_place() {
    assert_eq!(MEASUREMENT, 720_896);
}
