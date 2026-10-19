use super::*;

#[test]
#[ignore = "needs a display"]
fn shot_toolbar_widgets() {
    shot_display();

    let toolbar = ShotToolbar::new();
    let imp = toolbar.imp();
    assert!(imp.arrow.is_active() && imp.screen.is_active() && imp.red.is_active());
    assert_eq!(imp.strokes.active_name().as_deref(), Some("medium"));
    assert!(!imp.undo.is_sensitive() && !imp.redo.is_sensitive());

    let tools = Rc::new(RefCell::new(Vec::<String>::new()));
    let targets = Rc::new(RefCell::new(Vec::<String>::new()));
    let colors = Rc::new(RefCell::new(Vec::<u32>::new()));
    let strokes = Rc::new(RefCell::new(Vec::<u32>::new()));
    let actions = Rc::new(RefCell::new(Vec::<String>::new()));
    toolbar.connect_tool_selected({
        let tools = Rc::clone(&tools);
        move |_, tool| tools.borrow_mut().push(tool)
    });
    toolbar.connect_target_selected({
        let targets = Rc::clone(&targets);
        move |_, target| targets.borrow_mut().push(target)
    });
    toolbar.connect_color_selected({
        let colors = Rc::clone(&colors);
        move |_, color| colors.borrow_mut().push(color)
    });
    toolbar.connect_stroke_selected({
        let strokes = Rc::clone(&strokes);
        move |_, stroke| strokes.borrow_mut().push(stroke)
    });
    toolbar.connect_action({
        let actions = Rc::clone(&actions);
        move |_, action| actions.borrow_mut().push(action)
    });

    imp.arrow.emit_clicked();
    toolbar.set_tool("arrow");
    toolbar.set_tool("nope");
    assert!(
        tools.borrow().is_empty(),
        "the active tool and a setter emit nothing"
    );
    imp.crop.emit_clicked();
    assert_eq!(tools.borrow().as_slice(), ["crop"]);
    imp.crop.emit_clicked();
    assert_eq!(
        tools.borrow().len(),
        1,
        "a second press on the active tool is silent"
    );
    toolbar.set_tool("rect");
    assert_eq!(tools.borrow().len(), 1, "set_tool does not emit");
    assert!(imp.rect.is_active());

    imp.screen.emit_clicked();
    toolbar.set_target("screen");
    assert!(targets.borrow().is_empty());
    imp.window.emit_clicked();
    assert_eq!(targets.borrow().as_slice(), ["window"]);
    toolbar.set_target("screen");
    assert!(imp.screen.is_active());
    assert_eq!(targets.borrow().len(), 1);

    imp.red.emit_clicked();
    toolbar.set_color(0);
    toolbar.set_color(9);
    assert!(colors.borrow().is_empty());
    imp.blue.emit_clicked();
    assert_eq!(colors.borrow().as_slice(), [3]);
    toolbar.set_color(4);
    assert!(imp.white.is_active());
    assert_eq!(colors.borrow().len(), 1);

    toolbar.set_stroke(2);
    toolbar.set_stroke(0);
    assert!(strokes.borrow().is_empty());
    imp.strokes.set_active_name(Some("thick"));
    assert_eq!(strokes.borrow().as_slice(), [3]);
    toolbar.set_stroke(1);
    assert_eq!(imp.strokes.active_name().as_deref(), Some("thin"));
    assert_eq!(strokes.borrow().len(), 1);

    imp.undo.emit_clicked();
    imp.redo.emit_clicked();
    assert!(
        actions.borrow().is_empty(),
        "undo and redo start insensitive"
    );
    toolbar.set_can_undo(false);
    toolbar.set_can_redo(false);
    toolbar.set_can_undo(true);
    toolbar.set_can_redo(true);
    imp.undo.emit_clicked();
    imp.redo.emit_clicked();
    imp.copy_close.emit_by_name::<()>("clicked", &[]);
    imp.cancel.emit_clicked();
    for name in shot_toolbar::FINISH_ACTIONS {
        gtk4::prelude::ActionGroupExt::activate_action(&imp.finishing, name, None);
    }
    assert_eq!(
        actions.borrow().as_slice(),
        [
            "undo",
            "redo",
            "copy-close",
            "cancel",
            "save",
            "save-as",
            "open-with",
            "open-window",
        ]
    );
    let menu = imp.copy_close.menu_model().expect("a finish menu");
    let entries: Vec<(String, String)> = (0..menu.n_items())
        .flat_map(|section| {
            let items = menu.item_link(section, "section").expect("one section");
            (0..items.n_items())
                .map(|index| {
                    let text = |name: &str| {
                        items
                            .item_attribute_value(index, name, None)
                            .and_then(|value| value.get::<String>())
                            .unwrap_or_default()
                    };
                    (text("action"), text("accel"))
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        entries,
        [
            ("shot.save".to_owned(), "<Shift>Return".to_owned()),
            ("shot.save-as".to_owned(), "<Control><Shift>s".to_owned()),
            ("shot.open-with".to_owned(), String::new()),
            ("shot.open-window".to_owned(), "<Control>e".to_owned()),
        ]
    );
    let enabled = || {
        shot_toolbar::FINISH_ACTIONS
            .into_iter()
            .filter(|name| gtk4::prelude::ActionGroupExt::is_action_enabled(&imp.finishing, name))
            .collect::<Vec<_>>()
    };
    assert_eq!(enabled(), shot_toolbar::FINISH_ACTIONS);
    toolbar.set_finish_actions(&["save", "save-as", "open-with"]);
    assert_eq!(
        enabled(),
        ["save", "save-as", "open-with"],
        "--json drops Open in Window"
    );
    toolbar.set_finish_actions(&[]);
    assert!(enabled().is_empty());

    imp.filled_rect.emit_clicked();
    assert_eq!(
        tools.borrow().last().map(String::as_str),
        Some("filled-rect")
    );
    toolbar.set_tool("rect");
    toolbar.set_tool("filled-rect");
    assert!(imp.filled_rect.is_active());
    assert_eq!(
        imp.filled_rect.tooltip_text().as_deref(),
        Some("Filled Rectangle (F)")
    );

    assert!(!imp.apply_crop.get_visible());
    toolbar.set_apply_visible(true);
    assert!(imp.apply_crop.get_visible());
    let icon = imp
        .apply_crop
        .child()
        .and_downcast::<gtk4::Image>()
        .expect("Apply Crop icon");
    assert_eq!(icon.icon_name().as_deref(), Some("object-select-symbolic"));
    assert!(gtk4::IconTheme::for_display(&toolbar.display()).has_icon("object-select-symbolic"));
    imp.apply_crop.emit_clicked();
    assert_eq!(
        actions.borrow().last().map(String::as_str),
        Some("apply-crop")
    );
    toolbar.set_apply_visible(false);

    toolbar.set_target_visible(false);
    toolbar.set_finish_visible(false);
    assert!(!imp.targets.get_visible() && !imp.target_gap.get_visible());
    assert!(!imp.finish.get_visible() && !imp.finish_gap.get_visible());
    let visible = Rc::new(Cell::new(0));
    imp.targets.connect_notify_local(Some("visible"), {
        let visible = Rc::clone(&visible);
        move |_, _| visible.set(visible.get() + 1)
    });
    toolbar.set_target_visible(false);
    toolbar.set_finish_visible(false);
    assert_eq!(visible.get(), 0);

    toolbar.set_history_visible(false);
    assert!(!imp.history.get_visible() && !imp.history_gap.get_visible());
    let history_writes = Rc::new(Cell::new(0));
    imp.history.connect_notify_local(Some("visible"), {
        let history_writes = Rc::clone(&history_writes);
        move |_, _| history_writes.set(history_writes.get() + 1)
    });
    toolbar.set_history_visible(false);
    assert_eq!(history_writes.get(), 0, "an unchanged group writes nothing");
    toolbar.set_history_visible(true);
    assert!(imp.history.get_visible());

    assert!(imp.targets.is_sensitive());
    let sensitivity = Rc::new(Cell::new(0));
    imp.targets.connect_notify_local(Some("sensitive"), {
        let sensitivity = Rc::clone(&sensitivity);
        move |_, _| sensitivity.set(sensitivity.get() + 1)
    });
    toolbar.set_target_sensitive(false);
    assert!(!imp.targets.is_sensitive());
    toolbar.set_target_sensitive(false);
    assert_eq!(
        sensitivity.get(),
        1,
        "an unchanged sensitivity writes nothing"
    );
    toolbar.set_target_sensitive(true);
    assert!(imp.targets.is_sensitive());

    assert!(!imp.handle.get_visible(), "the handle starts hidden");
    let shown = Rc::new(Cell::new(0));
    imp.handle.connect_visible_notify({
        let shown = Rc::clone(&shown);
        move |_| shown.set(shown.get() + 1)
    });
    toolbar.set_movable(true);
    toolbar.set_movable(true);
    assert!(imp.handle.get_visible());
    assert_eq!(shown.get(), 1, "an unchanged movable writes nothing");
    let moves = Rc::new(RefCell::new(Vec::<(f64, f64)>::new()));
    toolbar.connect_moved({
        let moves = Rc::clone(&moves);
        move |_, dx, dy| moves.borrow_mut().push((dx, dy))
    });
    toolbar.emit_by_name::<()>("moved", &[&3.0_f64, &-2.0_f64]);
    assert_eq!(moves.borrow().as_slice(), [(3.0, -2.0)]);
}
