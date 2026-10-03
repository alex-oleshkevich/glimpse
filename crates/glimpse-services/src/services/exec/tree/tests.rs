use std::sync::Arc;

use serde::de::IntoDeserializer;

use serde_json::{Map, Value, json};

use super::{
    Element, ElementKind, EntryProps, FaderProps, IndicatorProps, LabelProps, ROOT, RowProps,
    ScaleProps, Severity, Tree, Violation, de_finite_f64,
};

use crate::services::exec::wire::{FromApplet, Op, WireNode};

const HELLO: &str = include_str!("../../../../../../sdk/applet/fixtures/hello.ndjson");

const OPS: &str = include_str!("../../../../../../sdk/applet/fixtures/ops.ndjson");

const UNKNOWN: &str =
    include_str!("../../../../../../sdk/applet/fixtures/invalid/unknown-id.ndjson");

const CYCLE: &str = include_str!("../../../../../../sdk/applet/fixtures/invalid/cycle.ndjson");

const BAD_PARENT: &str =
    include_str!("../../../../../../sdk/applet/fixtures/invalid/bad-parent.ndjson");

const TOO_MANY_NODES: &str =
    include_str!("../../../../../../sdk/applet/fixtures/invalid/max-nodes.ndjson");

const TOO_MANY_CHILDREN: &str =
    include_str!("../../../../../../sdk/applet/fixtures/invalid/max-children.ndjson");

const GARBAGE: &str = include_str!("../../../../../../sdk/applet/fixtures/invalid/garbage.ndjson");

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

fn fader_of(element: &Element) -> &FaderProps {
    match element {
        Element::Fader(props) => props,
        other => panic!("expected a fader, got {other:?}"),
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
