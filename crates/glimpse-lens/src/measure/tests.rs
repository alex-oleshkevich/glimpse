use super::*;

const EXAMPLE: Segment = Segment {
    from: (412, 268),
    to: (626, 364),
};

#[test]
fn an_output_going_away_discards_what_was_already_confirmed() {
    assert!(confirmed(Some(Outcome::Invalidated)).is_empty());
    assert!(confirmed(None).is_empty());
    assert_eq!(
        confirmed(Some(Outcome::Ended(vec![EXAMPLE]))),
        vec![EXAMPLE]
    );
}

#[test]
fn the_last_distance_is_whole_pixels_and_the_last_segment_wins() {
    let back = Segment {
        from: (626, 364),
        to: (600, 100),
    };
    assert_eq!(last_distance(&[]), None);
    assert_eq!(last_distance(&[EXAMPLE]).as_deref(), Some("235"));
    assert_eq!(last_distance(&[EXAMPLE, back]).as_deref(), Some("265"));
}

#[test]
fn a_session_with_nothing_confirmed_prints_nothing() {
    assert_eq!(ndjson(&[]), None);
}

#[test]
fn each_segment_is_one_flat_line_in_confirmation_order() {
    let back = Segment {
        from: (626, 364),
        to: (600, 100),
    };
    let text = ndjson(&[EXAMPLE, back]).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    assert!(text.ends_with('\n'));
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0],
        serde_json::json!({
            "from_x": 412,
            "from_y": 268,
            "to_x": 626,
            "to_y": 364,
            "dx": 214,
            "dy": 96,
            "distance": 214f64.hypot(96.0),
            "angle": 96f64.atan2(214.0).to_degrees(),
        })
    );
    assert_eq!(lines[1]["dx"], -26);
    assert_eq!(lines[1]["dy"], -264);
}
