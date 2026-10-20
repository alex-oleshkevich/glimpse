use std::sync::Arc;

use serde::de::IntoDeserializer;

use serde_json::{Map, Value, json};

use super::{
    Element, ElementKind, EntryProps, IndicatorProps, LabelProps, ROOT, RowProps, ScaleProps,
    Severity, Tree, Violation, de_finite_f64,
};

use crate::services::exec::wire::{FromApplet, Op, WireNode};

const HELLO: &str = include_str!("../../../../../../sdk/sdk-ts/fixtures/hello.ndjson");

#[test]
fn snapshot_preserves_properties_order_and_updates() {
    let tree = Tree::default()
        .apply(vec![insert(
            ROOT,
            wire(
                1,
                "indicator",
                json!({"icon": "test", "tooltip": "first"}),
                Vec::new(),
            ),
            None,
        )])
        .unwrap();
    let first = tree.snapshot();
    assert_eq!(first["children"][0]["props"]["tooltip"], "first");
    assert_eq!(first["children"][0]["type"], "indicator");
    let tree = tree
        .apply(vec![Op::Set {
            id: 1,
            props: props(json!({"tooltip": "second"})),
            seq: Some(4),
        }])
        .unwrap();
    let next = tree.snapshot();
    assert_eq!(next["children"][0]["props"]["tooltip"], "second");
    assert_eq!(next["children"][0]["seq"], 4);
    assert_eq!(tree.snapshot(), next);
    assert_eq!(first["children"][0]["props"]["tooltip"], "first");
    let fixture = lines(HELLO)
        .find(|line| line.contains("\"commit\""))
        .unwrap();
    let FromApplet::Commit { ops } = serde_json::from_str(fixture).unwrap() else {
        panic!("commit")
    };
    let fixture = Tree::default().apply(ops).unwrap().snapshot();
    assert_eq!(fixture["children"][0]["id"], 1);
    assert_eq!(fixture["children"][1]["id"], 2);
    assert_eq!(
        fixture["children"][1]["children"][0]["children"][0]["id"],
        4
    );
    assert_eq!(
        fixture["children"][1]["children"][0]["children"][1]["props"]["title"],
        "Ship applets"
    );
}

const OPS: &str = include_str!("../../../../../../sdk/sdk-ts/fixtures/ops.ndjson");

const UNKNOWN: &str =
    include_str!("../../../../../../sdk/sdk-ts/fixtures/invalid/unknown-id.ndjson");

const CYCLE: &str = include_str!("../../../../../../sdk/sdk-ts/fixtures/invalid/cycle.ndjson");

const BAD_PARENT: &str =
    include_str!("../../../../../../sdk/sdk-ts/fixtures/invalid/bad-parent.ndjson");

const TOO_MANY_NODES: &str =
    include_str!("../../../../../../sdk/sdk-ts/fixtures/invalid/max-nodes.ndjson");

const TOO_MANY_CHILDREN: &str =
    include_str!("../../../../../../sdk/sdk-ts/fixtures/invalid/max-children.ndjson");

const GARBAGE: &str = include_str!("../../../../../../sdk/sdk-ts/fixtures/invalid/garbage.ndjson");

fn lines(source: &str) -> impl Iterator<Item = &str> {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
}

fn commits(source: &str) -> Vec<Vec<Op>> {
    lines(source)
        .map(
            |line| match serde_json::from_str::<FromApplet>(line).expect("commit") {
                FromApplet::Commit { ops } => ops,
                other => panic!("expected a commit, got {other:?}"),
            },
        )
        .collect()
}

fn props(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn wire(id: u32, kind: &str, value: Value, children: Vec<WireNode>) -> WireNode {
    WireNode {
        id,
        kind: kind.to_owned(),
        props: props(value),
        children,
    }
}

fn leaf(id: u32, kind: &str) -> WireNode {
    wire(id, kind, json!({}), Vec::new())
}

fn insert(parent: u32, node: WireNode, before: Option<u32>) -> Op {
    Op::Insert {
        parent,
        node,
        before,
    }
}

fn row_of(element: &Element) -> &RowProps {
    match element {
        Element::Row(props) => props,
        other => panic!("expected a row, got {other:?}"),
    }
}

fn indicator_of(element: &Element) -> &IndicatorProps {
    match element {
        Element::Indicator(props) => props,
        other => panic!("expected an indicator, got {other:?}"),
    }
}

fn label_of(element: &Element) -> &LabelProps {
    match element {
        Element::Label(props) => props,
        other => panic!("expected a label, got {other:?}"),
    }
}

fn entry_of(element: &Element) -> &EntryProps {
    match element {
        Element::Entry(props) => props,
        other => panic!("expected an entry, got {other:?}"),
    }
}

fn hosted(tree: &Tree) -> Tree {
    tree.apply(vec![insert(ROOT, leaf(1, "popover"), None)])
        .expect("popover")
}

fn cleared_row(tree: &Tree) -> &RowProps {
    row_of(&tree.node(3).expect("row").element)
}

fn walk(tree: &Tree, id: u32) -> usize {
    1 + tree
        .children(id)
        .iter()
        .map(|child| walk(tree, *child))
        .sum::<usize>()
}

fn only(tree: &Tree, id: u32) -> Element {
    tree.node(id).expect("node").element.clone()
}

mod batches;
mod props;
mod rows;
mod widgets;

#[test]
fn an_oversized_string_prop_is_refused_on_insert_and_on_set() {
    let huge = "x".repeat(500_000);
    let refused = Tree::default().apply(vec![insert(
        ROOT,
        wire(1, "indicator", json!({"tooltip": huge}), Vec::new()),
        None,
    )]);
    assert_eq!(refused.unwrap_err(), Violation::StringTooLong);

    let tree = Tree::default()
        .apply(vec![insert(
            ROOT,
            wire(
                1,
                "indicator",
                json!({"tooltip": "x".repeat(4096)}),
                Vec::new(),
            ),
            None,
        )])
        .unwrap();
    let refused = tree.apply(vec![Op::Set {
        id: 1,
        props: props(json!({"tooltip": huge})),
        seq: None,
    }]);
    assert_eq!(refused.unwrap_err(), Violation::StringTooLong);
}
