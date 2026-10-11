use super::*;

#[test]
#[ignore = "needs a display"]
fn shot_capture_page_widgets() {
    if !shot_display() {
        return;
    }

    let page = ShotCapturePage::new();
    let imp = page.imp();
    assert_eq!(imp.modes.active_name().as_deref(), Some("screen"));
    assert_eq!(imp.modes.halign(), gtk4::Align::Fill);
    assert!(!imp.screen.get_visible() && !imp.window.get_visible());
    assert!(!imp.screen.uses_markup() && !imp.window.uses_markup());
    assert!(!imp.menu.get_visible());
    assert!(imp.menu.is_primary());
    assert_eq!(imp.menu.icon_name().as_deref(), Some("open-menu-symbolic"));
    assert_eq!(imp.menu.tooltip_text().as_deref(), Some("Main Menu"));

    let modes = Rc::new(RefCell::new(Vec::<String>::new()));
    let delays = Rc::new(RefCell::new(Vec::<u32>::new()));
    let cursors = Rc::new(RefCell::new(Vec::<bool>::new()));
    let takes = Rc::new(Cell::new(0));
    page.connect_mode_changed({
        let modes = Rc::clone(&modes);
        move |_, mode| modes.borrow_mut().push(mode)
    });
    page.connect_delay_changed({
        let delays = Rc::clone(&delays);
        move |_, seconds| delays.borrow_mut().push(seconds)
    });
    page.connect_cursor_changed({
        let cursors = Rc::clone(&cursors);
        move |_, shown| cursors.borrow_mut().push(shown)
    });
    page.connect_take({
        let takes = Rc::clone(&takes);
        move |_| takes.set(takes.get() + 1)
    });

    page.set_mode("screen");
    page.set_mode("nope");
    assert!(modes.borrow().is_empty());
    imp.modes.set_active_name(Some("window"));
    assert_eq!(modes.borrow().as_slice(), ["window"]);
    page.set_mode("window");
    assert_eq!(modes.borrow().len(), 1);

    page.set_outputs(&["eDP-1".to_owned()], "eDP-1");
    assert!(!imp.screen.get_visible(), "one output hides the Screen row");
    page.set_outputs(&["DP-2".to_owned(), "eDP-1".to_owned()], "eDP-1");
    assert!(
        !imp.screen.get_visible(),
        "window mode keeps the Screen row hidden"
    );
    page.set_mode("screen");
    assert!(imp.screen.get_visible());
    assert_eq!(page.selected_output(), "current");
    let screens = imp
        .screen
        .model()
        .expect("a screen model")
        .downcast::<gtk4::StringList>()
        .expect("screen names");
    assert_eq!(screens.string(0).as_deref(), Some("Current Screen"));
    assert_eq!(screens.string(1).as_deref(), Some("DP-2"));
    assert_eq!(screens.string(2).as_deref(), Some("eDP-1"));
    imp.screen.set_selected(0);
    assert_eq!(page.selected_output(), "current");
    imp.screen.set_selected(1);
    assert_eq!(page.selected_output(), "DP-2");
    imp.screen.set_selected(0);
    page.set_outputs(&["DP-2".to_owned(), "eDP-1".to_owned()], "eDP-1");
    assert_eq!(page.selected_output(), "current");

    let hostile = format!("Q&A <b>draft</b>{}", "é".repeat(190));
    page.set_windows(&[(4, hostile.clone()), (5, "glimpse — design.md".to_owned())]);
    assert!(!imp.window.get_visible());
    page.set_mode("window");
    assert!(imp.window.get_visible());
    let model = imp.window.model().expect("a window model");
    let strings = model.downcast::<gtk4::StringList>().expect("window titles");
    assert_eq!(strings.string(0).expect("a title").chars().count(), 200);
    assert!(
        strings
            .string(0)
            .expect("a title")
            .contains("Q&A <b>draft</b>")
    );
    imp.window.set_selected(1);
    assert_eq!(page.selected_window(), Some(5));
    page.set_windows(&[(4, hostile.clone()), (5, "glimpse — design.md".to_owned())]);
    page.set_windows(&[
        (1, "A".to_owned()),
        (5, "B".to_owned()),
        (9, "C".to_owned()),
    ]);
    assert_eq!(page.selected_window(), Some(5));
    page.set_windows(&[
        (9, "C".to_owned()),
        (1, "A".to_owned()),
        (5, "B".to_owned()),
    ]);
    assert_eq!(page.selected_window(), Some(5));

    page.set_delay(0);
    assert!(delays.borrow().is_empty());
    imp.delay.set_value(4.0);
    assert_eq!(delays.borrow().as_slice(), [4]);
    page.set_delay(4);
    page.set_delay(99);
    assert_eq!(imp.delay.value(), 60.0);
    assert_eq!(delays.borrow().len(), 1, "set_delay does not emit");

    page.set_cursor(false);
    assert!(cursors.borrow().is_empty());
    imp.cursor.set_active(true);
    assert_eq!(cursors.borrow().as_slice(), [true]);
    page.set_cursor(true);
    assert_eq!(cursors.borrow().len(), 1);

    imp.take.emit_clicked();
    assert_eq!(takes.get(), 1);

    let menu_writes = Rc::new(Cell::new(0));
    imp.menu.connect_notify_local(Some("menu-model"), {
        let menu_writes = Rc::clone(&menu_writes);
        move |_, _| menu_writes.set(menu_writes.get() + 1)
    });
    let model = gtk4::gio::Menu::new();
    page.set_menu_model(None);
    assert_eq!(menu_writes.get(), 0);
    page.set_menu_model(Some(model.upcast_ref()));
    assert!(imp.menu.get_visible());
    assert_eq!(menu_writes.get(), 1);
    page.set_menu_model(Some(model.upcast_ref()));
    assert_eq!(menu_writes.get(), 1, "the same menu writes nothing");
    page.set_menu_model(None);
    assert!(!imp.menu.get_visible());
    page.set_outputs_labeled(
        &[
            ("DP-2".to_owned(), "Dell U2723QE (DP-2)".to_owned()),
            ("eDP-1".to_owned(), "Built-in Display (eDP-1)".to_owned()),
        ],
        "eDP-1",
    );
    page.set_mode("screen");
    assert!(imp.targets.get_visible());
    let names = imp
        .screen
        .model()
        .expect("screen model")
        .downcast::<gtk4::StringList>()
        .expect("labels");
    assert_eq!(names.string(1).as_deref(), Some("Dell U2723QE (DP-2)"));
    let writes = Rc::new(Cell::new(0));
    imp.screen.connect_notify_local(Some("model"), {
        let writes = Rc::clone(&writes);
        move |_, _| writes.set(writes.get() + 1)
    });
    page.set_outputs_labeled(
        &[
            ("DP-2".to_owned(), "Dell U2723QE (DP-2)".to_owned()),
            ("eDP-1".to_owned(), "Built-in Display (eDP-1)".to_owned()),
        ],
        "eDP-1",
    );
    assert_eq!(writes.get(), 0);
    page.set_mode("region");
    assert!(!imp.targets.get_visible());
}
