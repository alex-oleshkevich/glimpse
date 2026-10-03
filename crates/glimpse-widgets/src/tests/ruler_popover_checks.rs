use super::*;

pub(super) fn check() {
    let ruler = RulerPopover::new();
    let ruler_imp = ruler.imp();
    let measure_requested = Rc::new(Cell::new(0));
    ruler.connect_measure_requested({
        let measure_requested = Rc::clone(&measure_requested);
        move |_| measure_requested.set(measure_requested.get() + 1)
    });
    ruler_imp.measure.activate();
    assert_eq!(measure_requested.get(), 1);

    assert!(
        !ruler_imp.history_section.get_visible(),
        "an untouched popover shows no history section at all"
    );
    let entries = [
        HistoryEntry {
            id: 1,
            title: "365.7px".to_owned(),
            value: "342 \u{d7} 128 px \u{b7} 20.6\u{b0}".to_owned(),
        },
        HistoryEntry {
            id: 2,
            title: "118.4px".to_owned(),
            value: "118 \u{d7} 0 px \u{b7} 0.0\u{b0}".to_owned(),
        },
    ];
    ruler.set_history(&entries);
    assert!(
        ruler_imp.history_section.get_visible(),
        "a non-empty history shows the section"
    );
    assert_eq!(children(&*ruler_imp.history_rows), 2);

    let first_row = ruler_imp
        .history_rows
        .first_child()
        .and_downcast::<Row>()
        .expect("a history row");
    assert_eq!(first_row.title().as_deref(), Some("365.7px"));
    assert_eq!(first_row.typography(), RowTypography::Monospace);
    assert_eq!(
        first_row.value().as_deref(),
        Some("342 \u{d7} 128 px \u{b7} 20.6\u{b0}")
    );

    let renders_after_first = ruler_imp.renders.get();
    let held_row = ruler_imp.history_held.borrow()[0].1.clone();
    ruler.set_history(&entries);
    assert_eq!(
        ruler_imp.renders.get(),
        renders_after_first,
        "an unchanged history list must never re-enter render_history at all"
    );
    assert_eq!(
        ruler_imp.history_held.borrow()[0].1,
        held_row,
        "an unchanged history list reuses its rows rather than rebuilding them"
    );

    let activated = Rc::new(RefCell::new(None));
    ruler.connect_activated({
        let activated = Rc::clone(&activated);
        move |_, id| {
            activated.replace(Some(id));
        }
    });
    first_row.activate();
    assert_eq!(
        *activated.borrow(),
        Some(1),
        "clicking a history row reports its own id"
    );

    ruler.set_history(&[]);
    assert!(
        !ruler_imp.history_section.get_visible(),
        "an emptied history hides the section again, matching the approved board's empty state"
    );
    assert_eq!(children(&*ruler_imp.history_rows), 0);

    ruler.set_footer(Some("Settings"));
    assert!(ruler_imp.footer.get_visible());
    assert_eq!(
        ruler_imp.footer.upcast_ref::<Row>().title().as_deref(),
        Some("Settings")
    );

    let footer_activated = Rc::new(Cell::new(false));
    ruler.connect_footer_activated({
        let footer_activated = Rc::clone(&footer_activated);
        move |_| footer_activated.set(true)
    });
    ruler_imp.footer.activate();
    assert!(footer_activated.get());
}
