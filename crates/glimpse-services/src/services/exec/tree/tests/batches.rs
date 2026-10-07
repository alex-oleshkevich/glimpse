use super::*;

#[test]
fn an_addon_can_be_replaced_insert_first() {
    let tree = hosted(&Tree::default())
        .apply(vec![insert(1, leaf(2, "addon"), None)])
        .expect("addon");
    let replaced = tree
        .apply(vec![
            insert(1, leaf(3, "addon"), None),
            Op::Remove { parent: 1, id: 2 },
        ])
        .expect("replace");
    assert_eq!(replaced.children(1), &[3]);
}

#[test]
fn too_many_ops_is_a_violation() {
    let ops = (0..=super::super::MAX_OPS)
        .map(|_| Op::Set {
            id: 1,
            props: Map::new(),
            seq: None,
        })
        .collect();
    assert_eq!(
        hosted(&Tree::default()).apply(ops).expect_err("cap"),
        Violation::TooManyOps
    );
}

#[test]
fn move_reorders_remove_drops_subtree_insert_honors_before() {
    let mut tree = Tree::default();
    for ops in commits(OPS) {
        tree = tree.apply(ops).expect("ops fixture");
    }
    assert_eq!(tree.children(ROOT), &[1]);
    assert_eq!(tree.children(1), &[2]);
    assert_eq!(tree.children(2), &[5, 3, 6, 4]);
    assert!(tree.node(7).is_none());
    assert!(tree.node(8).is_none());
    assert!(tree.node(9).is_none());
    let row = cleared_row(&tree);
    assert_eq!(row.title, None);
    assert!(!tree.node(3).expect("row").raw.contains_key("busy"));
    assert_eq!(tree.node(3).expect("row").seq, None);
    assert_eq!(
        row_of(&tree.node(5).expect("row").element).title.as_deref(),
        Some("c")
    );
}

#[test]
fn hello_fixture_builds_the_first_tree() {
    let mut tree = Tree::default();
    for line in lines(HELLO) {
        if let FromApplet::Commit { ops } = serde_json::from_str(line).expect("line") {
            tree = tree.apply(ops).expect("hello commit");
        }
    }
    assert_eq!(tree.children(ROOT), &[1, 2]);
    assert_eq!(tree.children(2), &[3]);
    assert_eq!(tree.children(3), &[4, 5]);
    assert_eq!(
        indicator_of(&tree.node(1).expect("chip").element)
            .text
            .as_deref(),
        Some("2")
    );
    match &tree.node(2).expect("popover").element {
        Element::Popover(popover) => assert_eq!(popover.title.as_deref(), Some("Todos")),
        other => panic!("expected a popover, got {other:?}"),
    }
    assert_eq!(
        row_of(&tree.node(4).expect("row").element).title.as_deref(),
        Some("Buy milk")
    );
    match &tree.node(5).expect("actionrow").element {
        Element::ActionRow(row) => assert!(row.on_activate),
        other => panic!("expected an actionrow, got {other:?}"),
    }
}

#[test]
fn invalid_batches_leave_the_tree_unchanged() {
    let tree = Tree::default();
    let root = Arc::clone(tree.node(ROOT).expect("root"));
    let cases = [
        (UNKNOWN, Violation::UnknownId(9)),
        (CYCLE, Violation::Cycle { id: 2, parent: 3 }),
        (
            BAD_PARENT,
            Violation::BadParent {
                parent: ElementKind::Indicator,
                child: ElementKind::Row,
            },
        ),
        (TOO_MANY_NODES, Violation::TooManyNodes),
        (TOO_MANY_CHILDREN, Violation::TooManyChildren { parent: 2 }),
    ];
    for (source, expected) in cases {
        let mut batches = commits(source);
        let ops = batches.remove(0);
        let error = tree.apply(ops).expect_err("violation");
        assert_eq!(error, expected);
        assert!(Arc::ptr_eq(&root, tree.node(ROOT).expect("root")));
        assert!(tree.children(ROOT).is_empty());
    }
    let line = lines(GARBAGE).next().expect("garbage");
    assert!(serde_json::from_str::<FromApplet>(line).is_err());
}

#[test]
fn the_parent_table_rejects_the_combinations_it_names() {
    let tree = hosted(&Tree::default());
    let section = tree
        .apply(vec![insert(
            1,
            wire(2, "section", json!({}), vec![leaf(3, "row")]),
            None,
        )])
        .expect("section");
    assert_eq!(
        section
            .apply(vec![insert(2, leaf(4, "section"), None)])
            .expect_err("nested section"),
        Violation::BadParent {
            parent: ElementKind::Section,
            child: ElementKind::Section,
        }
    );
    let boxed = tree
        .apply(vec![insert(1, leaf(2, "box"), None)])
        .expect("box");
    assert!(
        boxed
            .apply(vec![insert(2, leaf(3, "section"), None)])
            .is_ok()
    );
    let button = tree
        .apply(vec![insert(
            1,
            wire(2, "box", json!({}), vec![leaf(3, "button")]),
            None,
        )])
        .expect("button");
    match &button.node(3).expect("button").element {
        Element::Button(button) => assert!(button.sensitive),
        other => panic!("expected a button, got {other:?}"),
    }
    assert_eq!(
        tree.apply(vec![insert(1, leaf(2, "addon"), None)])
            .and_then(|tree| tree.apply(vec![insert(2, leaf(3, "section"), None)]))
            .expect_err("section in an addon"),
        Violation::BadParent {
            parent: ElementKind::Addon,
            child: ElementKind::Section,
        }
    );
    assert_eq!(
        section
            .apply(vec![insert(3, leaf(4, "section"), None)])
            .expect_err("child of a row"),
        Violation::BadParent {
            parent: ElementKind::Row,
            child: ElementKind::Section,
        }
    );
    assert_eq!(
        tree.apply(vec![insert(1, leaf(2, "entry"), None)])
            .and_then(|tree| tree.apply(vec![insert(2, leaf(3, "label"), None)]))
            .expect_err("child of a leaf"),
        Violation::BadParent {
            parent: ElementKind::Entry,
            child: ElementKind::Label,
        }
    );
}

#[test]
fn two_hundred_children_fit_and_one_more_does_not() {
    let children = (3..=202).map(|id| leaf(id, "label")).collect();
    let tree = hosted(&Tree::default())
        .apply(vec![insert(1, wire(2, "box", json!({}), children), None)])
        .expect("200 labels");
    assert_eq!(tree.children(2).len(), 200);
    assert_eq!(
        tree.apply(vec![insert(2, leaf(203, "label"), None)])
            .expect_err("201st"),
        Violation::TooManyChildren { parent: 2 }
    );
}

#[test]
fn the_node_cap_counts_the_root() {
    let mut next = 2;
    let mut ops = Vec::new();
    let mut last_box = 0;
    for index in 0..2 {
        let box_id = next;
        next += 1;
        last_box = box_id;
        let count = if index == 0 { 200 } else { 96 };
        let mut children = Vec::with_capacity(count);
        for _ in 0..count {
            children.push(leaf(next, "label"));
            next += 1;
        }
        ops.push(insert(1, wire(box_id, "box", json!({}), children), None));
    }
    let tree = hosted(&Tree::default()).apply(ops).expect("300 nodes");
    assert_eq!(walk(&tree, ROOT), super::super::MAX_NODES);
    assert_eq!(
        tree.apply(vec![insert(last_box, leaf(next, "label"), None)])
            .expect_err("one past the cap"),
        Violation::TooManyNodes
    );
}

#[test]
fn scale_range_resets_when_min_is_not_below_max() {
    let tree = hosted(&Tree::default())
        .apply(vec![insert(
            1,
            wire(
                2,
                "scale",
                json!({"min": 5, "max": 5, "value": 9}),
                Vec::new(),
            ),
            None,
        )])
        .expect("scale");
    match &tree.node(2).expect("scale").element {
        Element::Scale(ScaleProps {
            min, max, value, ..
        }) => {
            assert_eq!((*min, *max, *value), (0.0, 1.0, 1.0));
        }
        other => panic!("expected a scale, got {other:?}"),
    }
}

#[test]
fn a_failed_batch_does_not_keep_an_earlier_op() {
    let tree = hosted(&Tree::default());
    let error = tree
        .apply(vec![
            insert(1, leaf(2, "row"), None),
            insert(2, leaf(3, "section"), None),
        ])
        .expect_err("leaf");
    assert_eq!(
        error,
        Violation::BadParent {
            parent: ElementKind::Row,
            child: ElementKind::Section,
        }
    );
    assert!(tree.node(2).is_none());
}
