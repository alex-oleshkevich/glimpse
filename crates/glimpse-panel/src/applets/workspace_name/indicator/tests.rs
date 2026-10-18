use super::*;

#[test]
fn target_follows_scroll_direction() {
    assert_eq!(target(Direction::Down), WorkspaceRef::Next);
    assert_eq!(target(Direction::Right), WorkspaceRef::Next);
    assert_eq!(target(Direction::Up), WorkspaceRef::Prev);
    assert_eq!(target(Direction::Left), WorkspaceRef::Prev);
}
