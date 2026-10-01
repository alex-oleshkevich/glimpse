use super::*;

#[test]
fn unconstrained_or_anchorless_points_pass_through_unchanged() {
    assert_eq!(constrain(None, (10, 20), true), (10, 20));
    assert_eq!(constrain(Some((5, 5)), (10, 20), false), (10, 20));
}

#[test]
fn constrained_snaps_to_whichever_axis_moved_further() {
    assert_eq!(constrain(Some((100, 100)), (140, 110), true), (140, 100));
    assert_eq!(constrain(Some((100, 100)), (110, 140), true), (100, 140));
}

#[test]
fn a_tie_locks_horizontal() {
    assert_eq!(constrain(Some((100, 100)), (140, 140), true), (140, 100));
}

#[test]
fn resizing_steps_in_both_directions_and_clamps_to_the_documented_range() {
    let grown = resized(100.0, -1.0);
    assert!(grown > 100.0);
    let shrunk = resized(100.0, 1.0);
    assert!(shrunk < 100.0);
    assert_eq!(resized(MIN_RADIUS, 1.0), MIN_RADIUS);
    assert_eq!(resized(MAX_RADIUS, -1.0), MAX_RADIUS);
    assert_eq!(resized(100.0, 0.0), 100.0);
}

#[test]
fn distance_is_straight_and_the_angle_is_measured_from_horizontal_without_a_sign() {
    let segment = Segment {
        from: (412, 268),
        to: (626, 364),
    };
    assert_eq!(segment.delta(), (214, 96));
    assert!((segment.distance() - 234.546).abs() < 0.001);
    assert!((segment.angle() - 24.1609).abs() < 0.001);

    let back = Segment {
        from: segment.to,
        to: segment.from,
    };
    assert_eq!(back.delta(), (-214, -96));
    assert_eq!(back.distance(), segment.distance());
    assert_eq!(back.angle(), segment.angle());

    let flat = Segment {
        from: (10, 5),
        to: (0, 5),
    };
    assert_eq!(flat.angle(), 0.0);
    let upright = Segment {
        from: (5, 10),
        to: (5, 0),
    };
    assert_eq!(upright.angle(), 90.0);
    let still = Segment {
        from: (5, 5),
        to: (5, 5),
    };
    assert_eq!((still.distance(), still.angle()), (0.0, 0.0));
}

#[test]
fn the_first_point_anchors_and_each_later_one_confirms_a_segment_and_chains_on() {
    let mut chain = Chain::default();
    chain.place(0, (1, 1));
    assert!(chain.confirmed().is_empty());
    assert_eq!(chain.total(), None);

    chain.place(0, (4, 5));
    chain.place(0, (4, 15));

    assert_eq!(
        chain.confirmed(),
        vec![
            Segment {
                from: (1, 1),
                to: (4, 5)
            },
            Segment {
                from: (4, 5),
                to: (4, 15)
            },
        ]
    );
    assert_eq!(chain.anchor_on(0), Some((4, 15)));
    assert_eq!(chain.total(), Some((15.0, 2)));
}

#[test]
fn leaving_an_output_drops_its_anchor_but_keeps_what_was_confirmed() {
    let mut chain = Chain::default();
    chain.place(0, (0, 0));
    chain.place(0, (3, 4));
    chain.left(1);
    assert_eq!(chain.anchor_on(0), Some((3, 4)));

    chain.left(0);
    assert_eq!(chain.anchor_on(0), None);
    chain.place(1, (9, 9));
    chain.place(1, (9, 19));

    assert_eq!(chain.on(0).len(), 1);
    assert_eq!(chain.on(1).len(), 1);
    assert_eq!(chain.confirmed().len(), 2);
}

#[test]
fn a_point_on_another_output_never_joins_the_anchor_across_outputs() {
    let mut chain = Chain::default();
    chain.place(0, (5, 5));
    chain.place(1, (6, 6));

    assert!(chain.confirmed().is_empty());
    assert_eq!(chain.anchor_on(1), Some((6, 6)));
}

#[test]
fn reset_discards_every_confirmed_segment_and_the_pending_anchor() {
    let mut chain = Chain::default();
    chain.place(0, (0, 0));
    chain.place(0, (3, 4));
    chain.place(0, (10, 10));

    chain.reset();

    assert!(chain.confirmed().is_empty());
    assert_eq!(chain.anchor_on(0), None);
    assert_eq!(chain.total(), None);
}

#[test]
fn the_readout_shows_the_position_idle_and_the_segment_once_anchored() {
    assert_eq!(
        readout(None, (412, 268), None),
        [Some("412, 268".to_owned()), None, None]
    );
    assert_eq!(
        readout(Some((412, 268)), (626, 364), None),
        [
            Some("Δx 214  Δy 96".to_owned()),
            Some("234.5px · 24.2°".to_owned()),
            None,
        ]
    );
    assert_eq!(
        readout(Some((412, 268)), (626, 364), Some((12.34, 1))),
        [
            Some("Δx 214  Δy 96".to_owned()),
            Some("segment 234.5px · 24.2°".to_owned()),
            Some("total 12.3px, 1 segment".to_owned()),
        ]
    );
    assert_eq!(
        readout(None, (1, 2), Some((195.3, 2)))[2],
        Some("total 195.3px, 2 segments".to_owned())
    );
}

#[test]
fn the_readout_sits_under_the_lens_and_flips_above_it_near_the_bottom_edge() {
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
}
