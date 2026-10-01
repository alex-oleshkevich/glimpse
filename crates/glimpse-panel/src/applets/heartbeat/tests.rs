use super::*;

#[test]
fn scrolling_walks_the_period_and_stops_at_the_ends() {
    assert_eq!(stepped(DEFAULT_PERIOD_MS, Direction::Up), Some(900));
    assert_eq!(stepped(DEFAULT_PERIOD_MS, Direction::Down), Some(1100));
    assert_eq!(stepped(MIN_PERIOD_MS, Direction::Up), None);
    assert_eq!(stepped(MAX_PERIOD_MS, Direction::Down), None);
}

#[test]
fn a_horizontal_scroll_leaves_the_period_alone() {
    assert_eq!(stepped(DEFAULT_PERIOD_MS, Direction::Left), None);
    assert_eq!(stepped(DEFAULT_PERIOD_MS, Direction::Right), None);
}
