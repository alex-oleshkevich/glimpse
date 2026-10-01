use super::*;

fn all(runs: &[Run]) -> impl Iterator<Item = &(f32, f32)> {
    runs.iter().flatten()
}

#[test]
fn a_gap_splits_the_line() {
    let v = [Some(1.0), Some(2.0), None, Some(3.0), Some(4.0), Some(5.0)];
    let r = runs(&v, 100.0, 28.0);
    assert_eq!(r.iter().map(Vec::len).collect::<Vec<_>>(), vec![2, 3]);
    assert!(r[0][1].0 < r[1][0].0);
}

#[test]
fn points_stay_inside_the_padding() {
    let v: Vec<Option<f64>> = (0..366).map(|i| Some(((i * 37) % 11) as f64)).collect();
    for (w, h) in [(1.0, 1.0), (5.0, 28.0), (60.0, 28.0), (420.0, 28.0)] {
        for (x, y) in all(&runs(&v, w, h)) {
            assert!(x.is_finite() && y.is_finite());
            assert!(*x >= 0.0 && *x <= w, "x {x} in {w}");
            assert!(*y >= h * VERTICAL_PADDING - 1e-4 && *y <= h * (1.0 - VERTICAL_PADDING) + 1e-4);
        }
    }
}

#[test]
fn a_span_beyond_f64_stays_finite() {
    let r = runs(&[Some(-f64::MAX), Some(f64::MAX)], 100.0, 100.0);
    assert!(all(&r).all(|(x, y)| x.is_finite() && y.is_finite()));
    assert!((r[0][0].1 - 90.0).abs() < 1e-3);
    assert!((r[0][1].1 - 10.0).abs() < 1e-3);
}

#[test]
fn extremes_hit_the_padding_exactly() {
    let r = runs(&[Some(0.0), Some(10.0)], 100.0, 100.0);
    assert!((r[0][0].1 - 90.0).abs() < 1e-3);
    assert!((r[0][1].1 - 10.0).abs() < 1e-3);
}

#[test]
fn flat_series_is_centered() {
    let r = runs(&[Some(4.0), Some(4.0), Some(4.0)], 100.0, 28.0);
    assert!(all(&r).all(|p| p.1 == 14.0));
}

#[test]
fn nothing_to_draw_when_empty() {
    assert!(runs(&[None, None, None], 100.0, 28.0).is_empty());
    assert!(runs(&[], 100.0, 28.0).is_empty());
}

#[test]
fn isolated_points_are_single_point_runs() {
    let r = runs(&[Some(1.0), None, Some(2.0), None, Some(3.0)], 100.0, 28.0);
    assert_eq!(r.iter().map(Vec::len).collect::<Vec<_>>(), vec![1, 1, 1]);
    let one = runs(&[Some(7.0)], 100.0, 28.0);
    assert_eq!(one, vec![vec![(50.0, 14.0)]]);
}

#[test]
fn non_finite_values_are_gaps() {
    assert_eq!(clean(Some(f64::NAN)), None);
    assert_eq!(clean(Some(f64::NEG_INFINITY)), None);
    assert_eq!(clean(Some(-3.0)), Some(-3.0));
}
