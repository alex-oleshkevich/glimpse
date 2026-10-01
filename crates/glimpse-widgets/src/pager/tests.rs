use super::step;

#[test]
fn a_scroll_that_went_nowhere_is_not_a_step() {
    assert_eq!(step(0.0, 0.0), None);
}

#[test]
fn the_larger_delta_picks_the_axis() {
    assert_eq!(step(1.0, 0.2), Some((true, true)));
    assert_eq!(step(-1.0, 0.2), Some((true, false)));
    assert_eq!(step(0.2, 1.0), Some((false, true)));
    assert_eq!(step(0.2, -1.0), Some((false, false)));
}

#[test]
fn a_diagonal_of_equal_deltas_resolves_to_the_vertical_axis() {
    assert_eq!(step(1.0, 1.0), Some((false, true)));
    assert_eq!(step(-1.0, 1.0), Some((false, true)));
}

#[test]
fn one_axis_alone_never_reads_as_the_other() {
    assert_eq!(step(3.0, 0.0), Some((true, true)));
    assert_eq!(step(0.0, 3.0), Some((false, true)));
}
