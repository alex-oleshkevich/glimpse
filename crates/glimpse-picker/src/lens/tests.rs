use super::*;

#[test]
fn the_value_sits_under_the_lens_and_flips_above_it_near_the_bottom_edge() {
    let area = (1000.0, 800.0);
    let pill = (120.0, 34.0);

    assert_eq!(
        pill_origin((500.0, 300.0), 88.0, pill, area),
        (440.0, 398.0)
    );
    assert_eq!(
        pill_origin((500.0, 780.0), 88.0, pill, area),
        (440.0, 648.0)
    );
    assert_eq!(pill_origin((10.0, 300.0), 88.0, pill, area), (0.0, 398.0));
    assert_eq!(
        pill_origin((995.0, 300.0), 88.0, pill, area),
        (880.0, 398.0)
    );
    assert_eq!(
        pill_origin((500.0, 300.0), 106.0, pill, area),
        (440.0, 416.0)
    );
}

#[test]
fn scrolling_resizes_the_lens_and_stays_within_bounds() {
    assert_eq!(resized(100.0, -1.0), 112.0);
    assert_eq!(resized(100.0, 1.0), 88.0);
    assert_eq!(resized(100.0, 0.0), 100.0);
    assert_eq!(resized(RADIUS_MIN, 1.0), RADIUS_MIN);
    assert_eq!(resized(RADIUS_MAX, -1.0), RADIUS_MAX);
}
