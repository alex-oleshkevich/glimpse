use super::*;

pub(super) fn check() {
    let sources = SourceList::new();
    assert!(
        sources.first_child().is_none(),
        "a source list renders nothing before the first set"
    );

    let source = |key: &str, name: &str, value: f64, maximum: f64, floor: f64| Source {
        key: key.to_owned(),
        name: name.to_owned(),
        icon: String::new(),
        value,
        maximum,
        floor,
    };

    let changed = Rc::new(RefCell::new(Vec::<(String, f64)>::new()));
    sources.connect_changed({
        let changed = Rc::clone(&changed);
        move |_, key, value| changed.borrow_mut().push((key, value))
    });

    sources.set_sources(&[
        source("built-in", "Built-in", 40.0, 100.0, 0.0),
        source("dp-1", "DP-1", 60.0, 100.0, 0.0),
        source("dp-2", "DP-2", 80.0, 100.0, 5.0),
    ]);

    let rows: Vec<Row> = children_of(&sources);
    let faders: Vec<Fader> = children_of(&sources);
    assert_eq!(rows.len(), 3);
    assert_eq!(faders.len(), 3);
    assert_eq!(rows[0].title().as_deref(), Some("Built-in"));
    assert_eq!(rows[2].title().as_deref(), Some("DP-2"));
    assert!(
        !rows[0].activatable(),
        "a titled row above a fader takes no click of its own"
    );
    assert_eq!(
        rows[0].parent(),
        Some(sources.clone().upcast::<gtk4::Widget>())
    );
    assert_eq!(
        faders[0].parent(),
        Some(sources.clone().upcast::<gtk4::Widget>()),
        "the fader is a sibling of the row, not a child of it, so the row's own \
             non-activatable state cannot reach it"
    );

    assert_eq!(faders[0].value(), 40.0);
    assert_eq!(faders[2].value(), 80.0);

    assert_eq!(
        faders[2].floor(),
        5.0,
        "a source's own floor becomes the fader's floor"
    );
    assert_eq!(faders[2].maximum(), 100.0);

    faders[1].emit_by_name::<()>("changed", &[&65.0f64]);
    assert_eq!(
        *changed.borrow(),
        vec![("dp-1".to_owned(), 65.0)],
        "moving one fader reports that source's own key and no other"
    );
    changed.borrow_mut().clear();

    let first_row = rows[0].clone();
    let first_fader = faders[0].clone();
    sources.set_sources(&[
        source("built-in", "Built-in", 55.0, 100.0, 0.0),
        source("dp-1", "DP-1", 65.0, 100.0, 0.0),
        source("dp-2", "DP-2", 80.0, 100.0, 5.0),
    ]);
    let rows_after: Vec<Row> = children_of(&sources);
    let faders_after: Vec<Fader> = children_of(&sources);
    assert_eq!(
        rows_after[0], first_row,
        "an untouched row is reused rather than rebuilt"
    );
    assert_eq!(faders_after[0], first_fader, "and so is its fader");
    assert_eq!(faders_after[0].value(), 55.0);

    let renders_before = sources.imp().renders.get();
    sources.set_sources(&[
        source("built-in", "Built-in", 55.0, 100.0, 0.0),
        source("dp-1", "DP-1", 65.0, 100.0, 0.0),
        source("dp-2", "DP-2", 80.0, 100.0, 5.0),
    ]);
    assert_eq!(
        sources.imp().renders.get(),
        renders_before,
        "an identical slice returns early and never re-renders"
    );

    sources.set_sources(&[source("hostile", &"ё".repeat(300), 10.0, 100.0, 0.0)]);
    let hostile_rows: Vec<Row> = children_of(&sources);
    assert_eq!(hostile_rows.len(), 1);
    assert!(
        child_named::<gtk4::Label>(&hostile_rows[0], "row__title")
            .text()
            .chars()
            .count()
            <= TEXT_MAX_CHARS,
        "an unbounded, multi-byte source name is capped without slicing a character"
    );

    sources.set_sources(&[]);
    assert!(
        sources.first_child().is_none(),
        "an emptied source list renders no children"
    );
}
