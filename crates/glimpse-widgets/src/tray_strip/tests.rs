use super::visible_count;

#[test]
fn a_cap_of_zero_keeps_every_chip_on_the_strip() {
    assert_eq!(visible_count(5, 0), 5);
    assert_eq!(visible_count(0, 0), 0);
}

#[test]
fn a_cap_at_or_above_the_count_hides_nothing() {
    assert_eq!(visible_count(3, 3), 3);
    assert_eq!(visible_count(3, 9), 3);
}

#[test]
fn a_cap_below_the_count_leaves_the_rest_for_the_drawer() {
    assert_eq!(visible_count(5, 2), 2);
    assert_eq!(visible_count(5, 1), 1);
}
