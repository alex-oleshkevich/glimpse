use super::*;

fn all(runs: &[(bool, Run)]) -> impl Iterator<Item = &(f32, f32)> {
    runs.iter().flat_map(|(_, run)| run)
}

#[test]
fn a_gap_splits_the_line() {
    let v = [Some(1.0), Some(2.0), None, Some(3.0), Some(4.0), Some(5.0)];
    let r = runs(&v, 100.0, 28.0, None, &[]);
    assert_eq!(
        r.iter().map(|(_, run)| run.len()).collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert!(r[0].1[1].0 < r[1].1[0].0);
}

#[test]
fn points_stay_inside_the_padding() {
    let v: Vec<Option<f64>> = (0..366).map(|i| Some(((i * 37) % 11) as f64)).collect();
    for (w, h) in [(1.0, 1.0), (5.0, 28.0), (60.0, 28.0), (420.0, 28.0)] {
        for (x, y) in all(&runs(&v, w, h, None, &[])) {
            assert!(x.is_finite() && y.is_finite());
            assert!(*x >= 0.0 && *x <= w, "x {x} in {w}");
            assert!(*y >= h * VERTICAL_PADDING - 1e-4 && *y <= h * (1.0 - VERTICAL_PADDING) + 1e-4);
        }
    }
}

#[test]
fn a_span_beyond_f64_stays_finite() {
    let r = runs(&[Some(-f64::MAX), Some(f64::MAX)], 100.0, 100.0, None, &[]);
    assert!(all(&r).all(|(x, y)| x.is_finite() && y.is_finite()));
    assert!((r[0].1[0].1 - 90.0).abs() < 1e-3);
    assert!((r[0].1[1].1 - 10.0).abs() < 1e-3);
}

#[test]
fn extremes_hit_the_padding_exactly() {
    let r = runs(&[Some(0.0), Some(10.0)], 100.0, 100.0, None, &[]);
    assert!((r[0].1[0].1 - 90.0).abs() < 1e-3);
    assert!((r[0].1[1].1 - 10.0).abs() < 1e-3);
}

#[test]
fn flat_series_is_centered() {
    let r = runs(&[Some(4.0), Some(4.0), Some(4.0)], 100.0, 28.0, None, &[]);
    assert!(all(&r).all(|p| p.1 == 14.0));
}

#[test]
fn nothing_to_draw_when_empty() {
    assert!(runs(&[None, None, None], 100.0, 28.0, None, &[]).is_empty());
    assert!(runs(&[], 100.0, 28.0, None, &[]).is_empty());
}

#[test]
fn isolated_points_are_single_point_runs() {
    let r = runs(
        &[Some(1.0), None, Some(2.0), None, Some(3.0)],
        100.0,
        28.0,
        None,
        &[],
    );
    assert_eq!(
        r.iter().map(|(_, run)| run.len()).collect::<Vec<_>>(),
        vec![1, 1, 1]
    );
    let one = runs(&[Some(7.0)], 100.0, 28.0, None, &[]);
    assert_eq!(one, vec![(false, vec![(50.0, 14.0)])]);
}

#[test]
fn non_finite_values_are_gaps() {
    assert_eq!(clean(Some(f64::NAN)), None);
    assert_eq!(clean(Some(f64::NEG_INFINITY)), None);
    assert_eq!(clean(Some(-3.0)), Some(-3.0));
}

#[test]
fn fixed_domain_maps_and_clamps_values() {
    assert_eq!(value_y(50.0, 0.0, 100.0, 100.0, 0.0), 50.0);
    assert_eq!(value_y(120.0, 0.0, 100.0, 100.0, 0.0), 0.0);
    assert_eq!(value_y(-20.0, 0.0, 100.0, 100.0, 0.0), 100.0);
    let domain = Some((0.0, 100.0));
    let middle = runs(&[Some(50.0)], 100.0, 28.0, domain, &[]);
    assert_eq!(middle[0].1[0].0, 50.0);
    assert!((middle[0].1[0].1 - 14.0).abs() < 1e-4);
    let top = runs(&[Some(120.0)], 100.0, 28.0, domain, &[]);
    assert_eq!(top[0].1[0].0, 50.0);
    assert!((top[0].1[0].1 - STROKE / 2.0).abs() < 1e-4);
    for height in [1.0, 28.0, 100.0] {
        let r = runs(&[Some(-20.0), Some(120.0)], 100.0, height, domain, &[]);
        let inset = (STROKE / 2.0).min(height / 2.0);
        assert!((r[0].1[0].1 - (height - inset)).abs() < 1e-4);
        assert!((r[0].1[1].1 - inset).abs() < 1e-4);
    }
}

#[test]
fn tick_labels_center_on_their_ticks_and_clamp_at_the_edges() {
    for at in [0.19, 0.5, 0.81] {
        assert!(
            (tick_label_x(200.0, at, 20.0) + 10.0 - (EDGE + (200.0 - 2.0 * EDGE) * at as f32))
                .abs()
                < 1e-4
        );
    }
    assert_eq!(tick_label_x(200.0, 0.0, 20.0), EDGE);
    assert_eq!(tick_label_x(200.0, 1.0, 20.0), 200.0 - EDGE - 20.0);
    assert_eq!(tick_label_x(1.0, 0.5, 20.0), 0.0);
}

#[test]
fn tooltip_index_is_the_nearest_point() {
    for (width, count) in [(1, 1), (5, 3), (100, 5), (420, MAX_VALUES)] {
        for x in 0..width {
            let index = index_at(width, count, x).unwrap();
            let distance = (point_x(width as f32, count, index) - x as f32).abs();
            for other in 0..count {
                assert!(distance <= (point_x(width as f32, count, other) - x as f32).abs() + 1e-4);
            }
        }
    }
    assert_eq!(index_at(100, 5, 0), Some(0));
    assert_eq!(index_at(100, 5, 38), Some(1));
    assert_eq!(index_at(100, 5, 39), Some(2));
    assert_eq!(index_at(100, 5, 99), Some(4));
    assert_eq!(index_at(0, 5, 0), None);
    assert_eq!(index_at(100, 0, 50), None);
    assert_eq!(index_at(100, 5, -1), None);
    assert_eq!(index_at(100, 5, 100), None);
}

#[test]
fn empty_ticks_leave_the_measured_height_unchanged() {
    assert_eq!(
        measured_height(&[], 17),
        (MIN_HEIGHT, NATURAL_HEIGHT, -1, -1)
    );
}

#[test]
fn ticks_add_exactly_one_caption_line() {
    let ticks: Vec<Tick> = [0.0, 0.25, 0.5, 0.75]
        .into_iter()
        .map(|at| Tick {
            at,
            label: "16:00".into(),
        })
        .collect();
    for caption_height in [12, 17, 32] {
        assert_eq!(
            measured_height(&ticks, caption_height),
            (
                MIN_HEIGHT + caption_height,
                NATURAL_HEIGHT + caption_height,
                -1,
                -1
            )
        );
        assert_eq!(
            measured_height(&ticks[..1], caption_height),
            measured_height(&ticks, caption_height)
        );
    }
}

#[test]
fn marked_stretches_share_their_boundary_points() {
    let values = [Some(0.0), Some(25.0), Some(75.0), Some(100.0)];
    let marked = [false, true, true, false];
    let r = runs(&values, 100.0, 100.0, Some((0.0, 100.0)), &marked);
    assert_eq!(
        r.iter().map(|(mark, _)| *mark).collect::<Vec<_>>(),
        vec![false, true, false]
    );
    assert_eq!(r[0].1.last(), r[1].1.first());
    assert_eq!(r[1].1.last(), r[2].1.first());
    assert_eq!(r[1].1[1].0, point_x(100.0, 4, 1));
    assert!((r[1].1[1].1 - 74.625).abs() < 1e-4);
    assert_eq!(r[1].1[2].0, point_x(100.0, 4, 2));
    assert!((r[1].1[2].1 - 25.375).abs() < 1e-4);
}

#[test]
fn missing_marks_are_unmarked_and_gaps_stay_open() {
    let values = [Some(0.0), None, Some(50.0), Some(100.0)];
    let r = runs(&values, 100.0, 100.0, Some((0.0, 100.0)), &[true]);
    assert_eq!(
        r.iter().map(|(mark, _)| *mark).collect::<Vec<_>>(),
        vec![true, false]
    );
    assert_eq!(r[0].1.len(), 1);
    assert_eq!(r[1].1.len(), 2);
    assert!(r[0].1[0].0 < r[1].1[0].0);
}

#[test]
fn a_label_that_would_collide_with_now_is_dropped() {
    let placed = place_labels(300.0, &[(0.25, 30.0), (0.97, 30.0), (1.0, 24.0)]);
    assert!(placed[0].is_some());
    assert_eq!(placed[1], None);
    assert_eq!(placed[2], Some(tick_label_x(300.0, 1.0, 24.0)));
}

#[test]
fn labels_that_fit_are_all_kept() {
    let placed = place_labels(300.0, &[(0.25, 30.0), (0.5, 30.0), (1.0, 24.0)]);
    assert!(placed.iter().all(Option::is_some));
}
