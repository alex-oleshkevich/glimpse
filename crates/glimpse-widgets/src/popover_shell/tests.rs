use super::body_cap;

#[test]
fn body_cap_leaves_the_share_above_the_chrome_and_floors_at_zero() {
    assert_eq!(body_cap(1000, 100), 500);
    assert_eq!(body_cap(1000, 600), 0);
    assert_eq!(body_cap(1000, 700), 0);
}
