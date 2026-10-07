use super::*;

#[test]
fn widget_elements_decode_and_clean_their_props() {
    let long = "a".repeat(1000);
    let tree = hosted(&Tree::default())
        .apply(vec![
            insert(
                1,
                wire(
                    2,
                    "factlist",
                    json!({"facts": [{"label": long, "value": "v", "warning": true}, {"label": "b"}]}),
                    Vec::new(),
                ),
                None,
            ),
            insert(
                1,
                wire(
                    3,
                    "choicelist",
                    json!({"choices": [{"label": "x", "detail": "d", "icon": "/etc/passwd"}], "selected": 0, "onSelect": true}),
                    Vec::new(),
                ),
                None,
            ),
            insert(
                1,
                wire(4, "readout", json!({"value": "5", "unit": "C"}), Vec::new()),
                None,
            ),
            insert(
                1,
                wire(
                    5,
                    "notice",
                    json!({"title": "t", "severity": "error", "onActivate": true}),
                    Vec::new(),
                ),
                None,
            ),
            insert(
                1,
                wire(
                    6,
                    "calendar",
                    json!({"year": 100000, "month": 14, "selected": "2026-06-04", "today": "2026-02-30",
                           "events": [{"date": "2026-06-05", "colors": ["error", "accent"]}], "onMonth": true}),
                    Vec::new(),
                ),
                None,
            ),
            insert(
                1,
                wire(
                    7,
                    "dots",
                    json!({"colors": ["warning", "nope"], "size": 99, "max": 9, "uniform": true}),
                    Vec::new(),
                ),
                None,
            ),
            insert(
                1,
                wire(8, "dropdown", json!({"options": ["a", long], "selected": 1}), Vec::new()),
                None,
            ),
            insert(
                1,
                wire(9, "togglebutton", json!({"label": "l", "active": true}), Vec::new()),
                None,
            ),
            insert(1, wire(10, "checkbutton", json!({"label": "c"}), Vec::new()), None),
            insert(
                1,
                wire(11, "rangebar", json!({"min": 0, "max": 10, "low": 8, "high": 3, "now": 99}), Vec::new()),
                None,
            ),
            insert(
                1,
                wire(14, "revealer", json!({"revealed": true, "transition": "none"}), vec![leaf(15, "label")]),
                None,
            ),
            insert(1, wire(16, "stack", json!({"visible": 1}), vec![leaf(17, "label"), leaf(18, "label")]), None),
        ])
        .expect("widgets");

    let Element::FactList(facts) = only(&tree, 2) else {
        panic!("factlist")
    };
    assert_eq!(facts.facts[0].label, glimpse_utils::clean(&long, 256));
    assert!(facts.facts[0].warning);
    assert_eq!(facts.facts[1].value, "");
    let Element::ChoiceList(choices) = only(&tree, 3) else {
        panic!("choicelist")
    };
    assert_eq!(choices.choices[0].detail.as_deref(), Some("d"));
    assert_eq!(choices.choices[0].icon, None);
    assert_eq!(choices.selected, Some(0));
    assert!(choices.on_select);
    let Element::Readout(readout) = only(&tree, 4) else {
        panic!("readout")
    };
    assert_eq!(readout.unit.as_deref(), Some("C"));
    let Element::Notice(notice) = only(&tree, 5) else {
        panic!("notice")
    };
    assert_eq!(notice.severity, Some(Severity::Error));
    assert!(notice.on_activate);
    let Element::Calendar(calendar) = only(&tree, 6) else {
        panic!("calendar")
    };
    assert_eq!(calendar.year, Some(9999));
    assert_eq!(calendar.month, Some(12));
    assert_eq!(
        calendar.selected,
        chrono::NaiveDate::from_ymd_opt(2026, 6, 4)
    );
    assert_eq!(calendar.today, None);
    assert_eq!(
        calendar.events[0].colors,
        vec![super::super::Tone::Error, super::super::Tone::Accent]
    );
    assert!(calendar.on_month && !calendar.on_select);
    let Element::Dots(dots) = only(&tree, 7) else {
        panic!("dots")
    };
    assert!(dots.colors.is_empty());
    assert_eq!(
        (dots.size, dots.max, dots.uniform),
        (Some(16.0), Some(3), true)
    );
    let Element::DropDown(drop_down) = only(&tree, 8) else {
        panic!("dropdown")
    };
    assert_eq!(drop_down.options[1], glimpse_utils::clean(&long, 256));
    let Element::ToggleButton(toggle) = only(&tree, 9) else {
        panic!("togglebutton")
    };
    assert!(toggle.active && !toggle.on_toggle);
    assert!(matches!(only(&tree, 10), Element::CheckButton(_)));
    let Element::RangeBar(bar) = only(&tree, 11) else {
        panic!("rangebar")
    };
    assert_eq!(
        (bar.min, bar.max, bar.low, bar.high, bar.now),
        (0.0, 10.0, 8.0, 8.0, Some(10.0))
    );
    let Element::Revealer(revealer) = only(&tree, 14) else {
        panic!("revealer")
    };
    assert_eq!(revealer.transition, super::super::Transition::None);
    let Element::Stack(stack) = only(&tree, 16) else {
        panic!("stack")
    };
    assert_eq!(stack.visible, 1);
    assert_eq!(tree.children(16), &[17, 18]);
    let low = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(2, "calendar", json!({"year": 0}), Vec::new()),
            None,
        )])
        .expect("low year");
    let Element::Calendar(low) = only(&low, 2) else {
        panic!("low year")
    };
    assert_eq!(low.year, Some(1));
}

#[test]
fn a_nan_range_bar_bound_falls_back_to_the_unit_range() {
    let tree = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(
                2,
                "rangebar",
                json!({"min": "NaN", "max": 4, "low": 9, "high": 1}),
                Vec::new(),
            ),
            None,
        )])
        .expect("bar");
    let Element::RangeBar(bar) = only(&tree, 2) else {
        panic!("rangebar")
    };
    assert_eq!((bar.min, bar.max), (0.0, 4.0));
    assert_eq!((bar.low, bar.high), (4.0, 4.0));
}

#[test]
fn series_props_take_366_items_and_default_to_highlighting() {
    let series = |count: usize| (0..count).map(|i| json!(i)).collect::<Vec<_>>();
    let tree = hosted(&Tree::default())
        .apply(vec![
            insert(
                1,
                wire(
                    2,
                    "bars",
                    json!({"values": series(366), "tooltips": vec!["t"; 366], "label": "L"}),
                    Vec::new(),
                ),
                None,
            ),
            insert(
                1,
                wire(3, "sparkline", json!({"values": [1, null, 3]}), Vec::new()),
                None,
            ),
        ])
        .expect("366 fit");
    let Element::Bars(bars) = only(&tree, 2) else {
        panic!("bars")
    };
    assert_eq!((bars.values.len(), bars.tooltips.len()), (366, 366));
    assert!(bars.highlight_last);
    let Element::Sparkline(line) = only(&tree, 3) else {
        panic!("sparkline")
    };
    assert_eq!(line.values, vec![Some(1.0), None, Some(3.0)]);
    assert!(!line.fill && !line.show_last);
    for kind in ["bars", "sparkline"] {
        assert_eq!(
            hosted(&Tree::default())
                .apply(vec![insert(
                    1,
                    wire(2, kind, json!({"values": series(367)}), Vec::new()),
                    None,
                )])
                .expect_err("367"),
            Violation::TooManyItems
        );
    }
}

#[test]
fn array_props_over_sixty_four_items_are_a_violation() {
    let many = |count: usize| {
        (0..count)
            .map(|index| json!({"label": index.to_string(), "value": "v"}))
            .collect::<Vec<_>>()
    };
    let tree = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(2, "factlist", json!({"facts": many(64)}), Vec::new()),
            None,
        )])
        .expect("64 fit");
    assert_eq!(
        hosted(&Tree::default())
            .apply(vec![insert(
                1,
                wire(2, "factlist", json!({"facts": many(65)}), Vec::new()),
                None,
            )])
            .expect_err("65 insert"),
        Violation::TooManyItems
    );
    assert_eq!(
        tree.apply(vec![Op::Set {
            id: 2,
            props: props(json!({"facts": many(65)})),
            seq: None,
        }])
        .expect_err("65 set"),
        Violation::TooManyItems
    );
    let colors = vec!["accent"; 65];
    assert_eq!(
        hosted(&Tree::default())
            .apply(vec![insert(
                1,
                wire(
                    3,
                    "calendar",
                    json!({"events": [{"date": "2026-06-04", "colors": colors}]}),
                    Vec::new(),
                ),
                None,
            )])
            .expect_err("nested"),
        Violation::TooManyItems
    );
}

#[test]
fn a_revealer_holds_any_number_of_children_and_a_stack_any() {
    let root = hosted(&Tree::default());
    assert!(
        root.apply(vec![insert(1, leaf(2, "revealer"), None)])
            .is_ok(),
        "empty"
    );
    let many = root
        .apply(vec![insert(
            1,
            wire(
                2,
                "revealer",
                json!({}),
                vec![leaf(3, "label"), leaf(4, "label")],
            ),
            None,
        )])
        .expect("two");
    assert!(many.apply(vec![Op::Remove { parent: 2, id: 3 }]).is_ok());
    assert_eq!(
        many.apply(vec![insert(2, leaf(5, "section"), None)])
            .expect_err("section"),
        Violation::BadParent {
            parent: ElementKind::Revealer,
            child: ElementKind::Section,
        }
    );
    assert!(root.apply(vec![insert(1, leaf(2, "stack"), None)]).is_ok());
    assert!(
        root.apply(vec![insert(
            1,
            wire(
                2,
                "stack",
                json!({}),
                vec![leaf(3, "label"), leaf(4, "row"), leaf(5, "box")]
            ),
            None,
        )])
        .is_ok()
    );
}

#[test]
fn addon_and_container_reject_bad_children_and_clamp_props() {
    let root = hosted(&Tree::default());
    let two = root.apply(vec![insert(
        1,
        wire(
            2,
            "addon",
            json!({}),
            vec![leaf(3, "readout"), leaf(4, "switch")],
        ),
        None,
    )]);
    assert_eq!(two, Err(Violation::ChildCount { parent: 2 }));

    let row = root.apply(vec![insert(
        1,
        wire(2, "addon", json!({}), vec![leaf(3, "row")]),
        None,
    )]);
    assert_eq!(
        row,
        Err(Violation::BadParent {
            parent: ElementKind::Addon,
            child: ElementKind::Row,
        })
    );

    let two_body = root.apply(vec![insert(
        1,
        wire(
            2,
            "container",
            json!({}),
            vec![leaf(3, "label"), leaf(4, "label")],
        ),
        None,
    )]);
    assert_eq!(two_body, Err(Violation::ChildCount { parent: 2 }));

    let section = root.apply(vec![insert(
        1,
        wire(2, "container", json!({}), vec![leaf(3, "section")]),
        None,
    )]);
    assert_eq!(
        section,
        Err(Violation::BadParent {
            parent: ElementKind::Container,
            child: ElementKind::Section,
        })
    );

    let under_row = root.apply(vec![insert(
        1,
        wire(2, "row", json!({}), vec![leaf(3, "container")]),
        None,
    )]);
    assert_eq!(
        under_row,
        Err(Violation::BadParent {
            parent: ElementKind::Row,
            child: ElementKind::Container,
        })
    );

    let clamped = root
        .apply(vec![insert(
            1,
            wire(
                2,
                "container",
                json!({"width": 99999, "height": -50, "background": "#ff0000"}),
                vec![],
            ),
            None,
        )])
        .expect("container");
    match only(&clamped, 2) {
        Element::Container(props) => {
            assert_eq!(props.width, Some(1024));
            assert_eq!(props.height, Some(-1));
            assert_eq!(props.background, None);
        }
        other => panic!("{other:?}"),
    }

    let sized = root
        .apply(vec![Op::Set {
            id: 1,
            props: props(json!({"size": "huge"})),
            seq: None,
        }])
        .expect("size");
    match only(&sized, 1) {
        Element::Popover(props) => assert_eq!(props.size, None),
        other => panic!("{other:?}"),
    }
}
