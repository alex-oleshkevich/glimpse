use super::*;
#[test]
fn placement_carries_panel_zone_output_and_orientation() {
    assert_eq!(
        placement(Position::Left, 32, Some("DP-2".into()), panel::Zone::End),
        Placement {
            output: Some("DP-2".into()),
            position: Edge::Left,
            orientation: Orientation::Vertical,
            zone: Zone::Right,
            size: 32
        }
    );
}
