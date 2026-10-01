use super::*;

pub(super) fn check() {
    let bars = Bars::new();
    assert!(
        bars.highlight_last(),
        "the newest bar is highlighted by default"
    );
    assert_eq!(bars.accessible_role(), gtk4::AccessibleRole::Img);
    bars.set_values(&[
        Some(1.0),
        None,
        Some(-2.0),
        Some(f64::NAN),
        Some(f64::INFINITY),
    ]);
    assert_eq!(
        bars.values(),
        vec![Some(1.0), None, None, None, None],
        "negative and non-finite values are no data"
    );
    let many: Vec<Option<f64>> = (0..500).map(|i| Some(i as f64)).collect();
    bars.set_values(&many);
    assert_eq!(bars.values().len(), 366, "a series is capped");
    assert_eq!(bars.values()[365], Some(499.0), "the cap keeps the newest");
    bars.set_maximum(Some(-1.0));
    assert_eq!(
        bars.maximum(),
        None,
        "a maximum that cannot scale falls back to autoscale"
    );
    bars.set_maximum(Some(10.0));
    assert_eq!(bars.maximum(), Some(10.0));
    bars.set_highlight_last(false);
    assert!(!bars.highlight_last());
    bars.set_label(&"é".repeat(500));
    assert_eq!(
        bars.label().chars().count(),
        120,
        "the label is capped by chars"
    );
    assert_eq!(
        bars.measure(gtk4::Orientation::Vertical, -1).1,
        40,
        "bars claim their natural height"
    );
    bars.set_values(&[Some(1.0), Some(2.0), Some(3.0), Some(4.0)]);
    bars.set_tooltips(&[
        Some("one".to_owned()),
        None,
        Some("<b>three</b>".to_owned()),
        Some("é".repeat(500)),
    ]);
    bars.allocate(200, 40, -1, None);
    assert!(bars.has_tooltip());
    let tooltip_at = |x: i32| bars.tooltip_target(x).map(|target| target.0);
    assert_eq!(tooltip_at(10).as_deref(), Some("one"));
    assert_eq!(tooltip_at(60), None, "a bar with no tooltip shows none");
    assert_eq!(
        tooltip_at(110).as_deref(),
        Some("<b>three</b>"),
        "tooltip text is plain, never markup"
    );
    assert_eq!(tooltip_at(199).expect("last").chars().count(), 128);
    assert_eq!(tooltip_at(200), None, "past the last bar there is nothing");
    let first = bars.tooltip_target(10).expect("first").1;
    let third = bars.tooltip_target(110).expect("third").1;
    assert!(first.x() < third.x(), "each bar anchors its own tooltip");
    assert!(first.x() <= 10 && 10 < first.x() + first.width());
    assert_eq!(first.height(), bars.height());
    bars.set_tooltips(&[]);
    assert_eq!(tooltip_at(10), None, "no tooltips, no tooltip");

    let spark = Sparkline::new();
    assert_eq!(spark.accessible_role(), gtk4::AccessibleRole::Img);
    spark.set_values(&[Some(1.0), None, Some(f64::NAN), Some(3.0)]);
    assert_eq!(spark.values(), vec![Some(1.0), None, None, Some(3.0)]);
    spark.set_values(&many);
    assert_eq!(spark.values().len(), 366, "a series is capped");
    assert_eq!(spark.values()[365], Some(499.0), "the cap keeps the newest");
    spark.set_fill(true);
    spark.set_show_last(true);
    assert!(spark.fill() && spark.show_last());
    spark.set_label(&"é".repeat(500));
    assert_eq!(
        spark.label().chars().count(),
        120,
        "the label is capped by chars"
    );
}
