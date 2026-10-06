use serde_json::json;

use super::{Element, Tree, Violation, hosted, insert, leaf, wire};

#[test]
fn new_row_parent_table_and_legacy_row() {
    for invalid in [
        leaf(2, "start"),
        wire(2, "actionrow", json!({}), vec![leaf(3, "button")]),
        wire(
            2,
            "actionrow",
            json!({}),
            vec![leaf(3, "image"), leaf(4, "spinner")],
        ),
        wire(2, "expanderrow", json!({}), vec![leaf(3, "section")]),
        wire(2, "togglerow", json!({}), vec![leaf(3, "image")]),
        wire(
            2,
            "expanderrow",
            json!({}),
            vec![leaf(3, "start"), leaf(4, "start")],
        ),
        wire(
            2,
            "expanderrow",
            json!({}),
            vec![wire(3, "start", json!({}), vec![leaf(4, "section")])],
        ),
        wire(
            2,
            "sliderrow",
            json!({}),
            vec![leaf(3, "start"), leaf(4, "start")],
        ),
        wire(
            2,
            "sliderrow",
            json!({}),
            vec![wire(
                3,
                "start",
                json!({}),
                vec![leaf(4, "button"), leaf(5, "image")],
            )],
        ),
        wire(
            2,
            "expanderrow",
            json!({}),
            vec![leaf(3, "end"), leaf(4, "end")],
        ),
    ] {
        assert!(matches!(
            hosted(&Tree::default()).apply(vec![insert(1, invalid, None)]),
            Err(Violation::BadParent { .. } | Violation::ChildCount { .. })
        ));
    }
    let tree = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(
                2,
                "sliderrow",
                json!({}),
                vec![wire(3, "start", json!({}), vec![leaf(4, "button")])],
            ),
            None,
        )])
        .expect("slider start");
    assert!(matches!(
        &tree.node(4).expect("button").element,
        Element::Button(_)
    ));
    for row in ["actionrow", "expanderrow"] {
        let tree = hosted(&Tree::default())
            .apply(vec![insert(
                1,
                wire(
                    2,
                    row,
                    json!({}),
                    vec![wire(3, "start", json!({}), vec![leaf(4, "dots")])],
                ),
                None,
            )])
            .expect("a row leads with dots");
        assert!(matches!(
            &tree.node(4).expect("dots").element,
            Element::Dots(_)
        ));
    }
    let tree = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(
                2,
                "row",
                json!({"selected": true, "onActivate": true}),
                vec![],
            ),
            None,
        )])
        .expect("legacy row");
    assert!(
        matches!(&tree.node(2).expect("row").element, Element::Row(props) if props.selected == Some(true) && props.on_activate)
    );
}

#[test]
fn new_row_props_are_bounded_and_normalized() {
    let long = "я".repeat(10_000);
    let props = json!({"icon":"folder-symbolic", "title":long, "subtitle":long, "value":long, "tooltip":long, "tone":"warning", "typography":"monospace"});
    let tree = hosted(&Tree::default())
        .apply(vec![insert(1, wire(2, "actionrow", props, vec![]), None)])
        .expect("row");
    let Element::ActionRow(props) = &tree.node(2).expect("row").element else {
        panic!("actionrow")
    };
    for value in [&props.title, &props.subtitle, &props.value, &props.tooltip] {
        assert!(value.as_deref().expect("text").chars().count() <= 256);
    }

    let tree = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(
                2,
                "meterrow",
                json!({"level": 2, "tone":"unknown", "typography":"unknown"}),
                vec![],
            ),
            None,
        )])
        .expect("meter");
    assert!(
        matches!(&tree.node(2).expect("meter").element, Element::MeterRow(props) if props.level == Some(1.0) && props.tone == super::super::RowTone::Normal && props.typography == super::super::RowTypography::Default)
    );

    let tree = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(
                2,
                "sliderrow",
                json!({"min": 2, "max": 1, "step": 0, "value": 9}),
                vec![],
            ),
            None,
        )])
        .expect("slider");
    assert!(
        matches!(&tree.node(2).expect("slider").element, Element::SliderRow(props) if props.min == 0.0 && props.max == 1.0 && props.step == 0.01 && props.value == Some(1.0))
    );

    let meter = super::super::finish_meter_row(super::super::MeterRowProps {
        level: Some(f64::NAN),
        ..Default::default()
    });
    assert_eq!(meter.level, None);
    let slider = super::super::finish_slider_row(super::super::SliderRowProps {
        value: Some(f64::INFINITY),
        min: f64::NAN,
        max: f64::INFINITY,
        step: f64::NAN,
        ..Default::default()
    });
    assert_eq!(
        (slider.value, slider.min, slider.max, slider.step),
        (Some(0.0), 0.0, 1.0, 0.01)
    );
}

#[test]
fn row_inactivity_is_boolean_and_defaults_to_false() {
    for tag in [
        "actionrow",
        "togglerow",
        "meterrow",
        "expanderrow",
        "sliderrow",
    ] {
        for props in [
            json!({}),
            json!({"inactive":false}),
            json!({"inactive":true}),
        ] {
            let expected = props
                .get("inactive")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let tree = hosted(&Tree::default())
                .apply(vec![insert(1, wire(2, tag, props, vec![]), None)])
                .expect("row");
            let inactive = match &tree.node(2).expect("row").element {
                Element::ActionRow(p) => p.inactive,
                Element::ToggleRow(p) => p.inactive,
                Element::MeterRow(p) => p.inactive,
                Element::ExpanderRow(p) => p.inactive,
                Element::SliderRow(p) => p.inactive,
                _ => panic!("row"),
            };
            assert_eq!(inactive, expected);
        }
    }
}

#[test]
fn expander_card_and_title_line_clamps() {
    for (input, expected) in [
        (json!(null), 1),
        (json!(0), 1),
        (json!(-1), 1),
        (json!(2.5), 1),
        (json!(3), 3),
        (json!(9), 4),
        (json!("3"), 1),
    ] {
        let tree = hosted(&Tree::default())
            .apply(vec![insert(
                1,
                wire(
                    2,
                    "expanderrow",
                    json!({"tint":true,"titleLines":input}),
                    vec![],
                ),
                None,
            )])
            .expect("expander");
        let Element::ExpanderRow(p) = &tree.node(2).expect("expander").element else {
            panic!("expander")
        };
        assert!(p.tint);
        assert_eq!(p.title_lines, expected);
    }
    let tree = hosted(&Tree::default())
        .apply(vec![insert(1, leaf(2, "expanderrow"), None)])
        .expect("defaults");
    let Element::ExpanderRow(p) = &tree.node(2).expect("expander").element else {
        panic!("expander")
    };
    assert!(!p.tint);
    assert_eq!(p.title_lines, 1);
    assert!(!p.inactive);
}

#[test]
fn retired_expander_tag_uses_unknown_tag_behavior() {
    let tree = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            leaf(2, concat!("notice", "expander")),
            None,
        )])
        .expect("unknown tag");
    assert!(
        matches!(&tree.node(2).expect("unsupported").element, Element::Unsupported(tag) if tag == concat!("notice", "expander"))
    );
}
