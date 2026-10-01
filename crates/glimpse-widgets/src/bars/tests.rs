use super::*;

#[test]
fn layout_never_leaves_the_width() {
    for width in 1..=600 {
        for count in 0..=400 {
            for gap in [0, 1, 2, 5] {
                let slots = layout(width, count, gap);
                assert_eq!(slots.len(), count.min(width as usize));
                let mut edge = 0;
                for (x, w) in &slots {
                    assert!(*w >= 1, "width {width} count {count}");
                    assert!(*x >= edge, "overlap at width {width} count {count}");
                    edge = x + w;
                }
                assert!(edge <= width, "width {width} count {count} gap {gap}");
            }
        }
    }
}

#[test]
fn layout_fills_the_width_and_keeps_the_gap_when_there_is_room() {
    let slots = layout(420, 30, 2);
    assert_eq!(slots[0].0, 0);
    let (x, w) = slots[29];
    assert_eq!(x + w, 420);
    assert_eq!(slots[1].0 - (slots[0].0 + slots[0].1), 2);
    assert_eq!(layout(0, 5, 2), vec![]);
    assert_eq!(layout(10, 0, 2), vec![]);
    assert_eq!(layout(10, 10, 2).iter().map(|s| s.1).sum::<i32>(), 10);
}

#[test]
fn bucketing_keeps_the_maximum_and_the_gaps() {
    let values = [Some(1.0), Some(5.0), None, None, Some(2.0), Some(1.0)];
    assert_eq!(bucket(&values, 3), vec![Some(5.0), None, Some(2.0)]);
    assert_eq!(bucket(&values, 6), values.to_vec());
    let long: Vec<Option<f64>> = (0..400).map(|i| Some(i as f64)).collect();
    let bucketed = bucket(&long, 7);
    assert_eq!(bucketed.len(), 7);
    assert_eq!(bucketed[6], Some(399.0));
}

#[test]
fn every_x_maps_to_a_value_or_nothing() {
    for width in 1..=200 {
        for count in (0..=400).step_by(3).chain([1, 2, 365, 366]) {
            let values: Vec<Option<f64>> = (0..count).map(|i| Some((i % 7) as f64)).collect();
            let slots = layout(width, count, gap_for(width, count));
            for x in -2..width + 2 {
                let hit = index_at(width, &values, x);
                if x < 0 || x >= width || count == 0 {
                    assert_eq!(hit, None);
                    continue;
                }
                let index = hit.expect("inside the bars");
                assert!(index < count, "width {width} count {count} x {x}");
                if let Some(slot) = slots.iter().position(|(at, w)| x >= *at && x < at + w) {
                    assert!(span(count, slots.len(), slot).contains(&index));
                }
            }
        }
    }
}

#[test]
fn a_bucket_reports_its_largest_value() {
    let values = [Some(1.0), Some(9.0), Some(3.0), Some(2.0)];
    assert_eq!(index_at(2, &values, 0), Some(1));
    assert_eq!(index_at(2, &values, 1), Some(2));
    let empty = [None, None, Some(1.0), None];
    assert_eq!(index_at(2, &empty, 0), Some(1));
    assert_eq!(index_at(4, &values, 2), Some(2));
}

#[test]
fn a_gap_maps_to_the_nearer_bar() {
    let values = vec![Some(1.0); 10];
    let slots = layout(100, 10, gap_for(100, 10));
    let (first_end, second_start) = (slots[0].0 + slots[0].1, slots[1].0);
    assert!(second_start > first_end);
    assert_eq!(index_at(100, &values, first_end), Some(0));
    assert_eq!(index_at(100, &values, second_start - 1), Some(1));
}

#[test]
fn heights_are_finite_and_bounded() {
    assert_eq!(bar_height(0.0, 10.0, 40.0), 1.0);
    assert_eq!(bar_height(0.001, 10.0, 40.0), 2.0);
    assert_eq!(bar_height(5.0, 10.0, 40.0), 20.0);
    assert_eq!(bar_height(99.0, 10.0, 40.0), 40.0);
    assert_eq!(bar_height(3.0, 0.0, 40.0), 40.0);
    assert!(bar_height(f64::MAX, f64::MIN_POSITIVE, 40.0).is_finite());
}

#[test]
fn invalid_values_become_none() {
    assert_eq!(clean(Some(-1.0)), None);
    assert_eq!(clean(Some(f64::NAN)), None);
    assert_eq!(clean(Some(f64::INFINITY)), None);
    assert_eq!(clean(Some(0.0)), Some(0.0));
}
