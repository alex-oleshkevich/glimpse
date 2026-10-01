use super::*;

#[test]
fn text_is_cleaned_icons_dropped_and_unsupported_names_are_cleaned() {
    let hostile = "<b>hi</b>\u{7}\u{202e}there";
    let long = "a".repeat(10_000);
    let type_name = "<b>x</b>\u{202e}";
    let tree = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(
                2,
                "row",
                json!({"title": hostile, "icon": "/etc/passwd", "value": "<b>x</b>"}),
                Vec::new(),
            ),
            None,
        )])
        .expect("row")
        .apply(vec![insert(
            1,
            wire(3, "label", json!({"text": long}), Vec::new()),
            None,
        )])
        .expect("label")
        .apply(vec![insert(1, leaf(4, type_name), None)])
        .expect("unsupported");

    let row = row_of(&tree.node(2).expect("row").element);
    assert_eq!(
        row.title.as_deref(),
        Some(glimpse_utils::clean(hostile, 256).as_str())
    );
    assert!(
        row.title
            .as_ref()
            .is_some_and(|title| title.contains("<b>hi</b>"))
    );
    assert_eq!(row.icon, None);
    assert_eq!(row.value.as_deref(), Some("<b>x</b>"));
    assert_eq!(
        label_of(&tree.node(3).expect("label").element)
            .text
            .as_deref(),
        Some(glimpse_utils::clean(&long, 256).as_str())
    );
    assert_eq!(
        tree.node(4).expect("unsupported").element,
        Element::Unsupported(glimpse_utils::clean(type_name, 64))
    );
}

#[test]
fn entry_value_keeps_spaces_and_strips_controls() {
    let tree = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(
                2,
                "entry",
                json!({"value": "  hello \u{7}\u{202e}"}),
                Vec::new(),
            ),
            None,
        )])
        .expect("entry");
    assert_eq!(
        entry_of(&tree.node(2).expect("entry").element).value,
        "  hello "
    );

    let long = format!("{} ", "a".repeat(4096));
    let tree = tree
        .apply(vec![Op::Set {
            id: 2,
            props: props(json!({"value": long})),
            seq: None,
        }])
        .expect("cap");
    let value = &entry_of(&tree.node(2).expect("entry").element).value;
    assert_eq!(value.chars().count(), 4096);
    assert!(value.ends_with('a'));
}

#[test]
fn a_nan_fader_value_is_dropped() {
    let nan: serde::de::value::F64Deserializer<serde::de::value::Error> =
        f64::NAN.into_deserializer();
    assert!(de_finite_f64(nan).is_err());
    let infinite: serde::de::value::F64Deserializer<serde::de::value::Error> =
        f64::INFINITY.into_deserializer();
    assert!(de_finite_f64(infinite).is_err());

    let tree = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(
                2,
                "fader",
                json!({"value": "NaN", "maximum": 40, "icon": "audio-volume-high-symbolic"}),
                Vec::new(),
            ),
            None,
        )])
        .expect("fader");
    let fader = fader_of(&tree.node(2).expect("fader").element);
    assert_eq!(fader.value, 0.0);
    assert_eq!(fader.maximum, 40.0);
    assert_eq!(fader.floor, 0.0);
    assert_eq!(fader.icon.as_deref(), Some("audio-volume-high-symbolic"));

    let bare = tree
        .apply(vec![insert(
            1,
            wire(3, "fader", json!({}), Vec::new()),
            None,
        )])
        .expect("defaults");
    let fader = fader_of(&bare.node(3).expect("fader").element);
    assert_eq!(fader.value, 0.0);
    assert_eq!(fader.maximum, 100.0);
    assert_eq!(fader.floor, 0.0);
}

#[test]
fn indicator_props_decode_badge_cap_and_bad_icons() {
    let tree = Tree::default()
        .apply(vec![insert(
            ROOT,
            wire(
                1,
                "indicator",
                json!({
                    "icon": "audio-volume-high-symbolic",
                    "overlay": "emblem-important-symbolic",
                    "dot": "#e01b24",
                    "badge": "123456789",
                    "severity": "warning",
                    "attention": true,
                    "notice": true,
                    "className": "accent",
                    "onPress": true,
                    "text": "2"
                }),
                Vec::new(),
            ),
            None,
        )])
        .expect("indicator");
    let props = indicator_of(&tree.node(1).expect("indicator").element);
    assert_eq!(props.icon.as_deref(), Some("audio-volume-high-symbolic"));
    assert_eq!(props.overlay.as_deref(), Some("emblem-important-symbolic"));
    assert_eq!(props.dot.as_deref(), Some("#e01b24"));
    assert_eq!(
        props.badge.as_deref(),
        Some(glimpse_utils::clean("123456789", 8).as_str())
    );
    assert_eq!(props.severity, Some(Severity::Warning));
    assert!(props.attention);
    assert!(props.notice);
    assert_eq!(props.class_name, Some(super::super::ClassName::Accent));
    assert!(props.on_press);
    assert_eq!(props.text.as_deref(), Some("2"));

    let tree = Tree::default()
        .apply(vec![insert(
            ROOT,
            wire(
                1,
                "indicator",
                json!({
                    "icon": "/etc/passwd",
                    "overlay": "bad icon",
                    "badge": "3",
                    "className": "nope",
                    "text": "ok"
                }),
                Vec::new(),
            ),
            None,
        )])
        .expect("dropped");
    let props = indicator_of(&tree.node(1).expect("indicator").element);
    assert_eq!(props.icon, None);
    assert_eq!(props.overlay, None);
    assert_eq!(props.class_name, None);
    assert_eq!(props.badge.as_deref(), Some("3"));
    assert_eq!(props.text.as_deref(), Some("ok"));
}

#[test]
fn set_keeps_other_arcs_deletes_null_and_drops_wrong_types() {
    let tree = hosted(&Tree::default())
        .apply(vec![
            insert(
                1,
                wire(2, "row", json!({"title": "Milk"}), Vec::new()),
                None,
            ),
            insert(
                1,
                wire(3, "row", json!({"title": "Bread"}), Vec::new()),
                None,
            ),
        ])
        .expect("rows");
    let kept = Arc::clone(tree.node(3).expect("other"));
    let updated = tree
        .apply(vec![Op::Set {
            id: 2,
            props: props(json!({"title": "Oat", "busy": "nope", "subtitle": "later"})),
            seq: None,
        }])
        .expect("set");
    assert!(Arc::ptr_eq(&kept, updated.node(3).expect("other")));
    assert!(!Arc::ptr_eq(
        tree.node(2).expect("before"),
        updated.node(2).expect("after")
    ));
    assert!(Arc::ptr_eq(
        tree.node(1).expect("parent"),
        updated.node(1).expect("parent")
    ));
    let row = row_of(&updated.node(2).expect("row").element);
    assert_eq!(row.title.as_deref(), Some("Oat"));
    assert_eq!(row.subtitle.as_deref(), Some("later"));
    assert!(!row.busy);
    assert!(!updated.node(2).expect("row").raw.contains_key("busy"));

    let cleared = updated
        .apply(vec![Op::Set {
            id: 2,
            props: props(json!({"title": null})),
            seq: Some(4),
        }])
        .expect("null");
    assert!(Arc::ptr_eq(&kept, cleared.node(3).expect("other")));
    let node = cleared.node(2).expect("row");
    assert!(!node.raw.contains_key("title"));
    assert_eq!(row_of(&node.element).title, None);
    assert_eq!(row_of(&node.element).subtitle.as_deref(), Some("later"));
    assert_eq!(node.seq, Some(4));

    let unsequenced = cleared
        .apply(vec![Op::Set {
            id: 2,
            props: props(json!({"title": "Rye"})),
            seq: None,
        }])
        .expect("unsequenced");
    assert_eq!(unsequenced.node(2).expect("row").seq, None);
}

#[test]
fn an_unknown_key_is_not_kept() {
    let tree = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(2, "row", json!({"title": "Milk", "junk": "x"}), Vec::new()),
            None,
        )])
        .expect("row")
        .apply(vec![Op::Set {
            id: 2,
            props: props(json!({"more": "y".repeat(1000)})),
            seq: None,
        }])
        .expect("set");
    let node = tree.node(2).expect("row");
    assert!(!node.raw.contains_key("junk"));
    assert!(!node.raw.contains_key("more"));
    assert_eq!(row_of(&node.element).title.as_deref(), Some("Milk"));
    let unsupported = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(3, "canvas", json!({"a": 1}), Vec::new()),
            None,
        )])
        .expect("unsupported");
    assert!(unsupported.node(3).expect("canvas").raw.is_empty());
}
