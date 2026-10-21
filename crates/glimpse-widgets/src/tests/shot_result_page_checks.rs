use super::*;

#[test]
#[ignore = "needs a display"]
fn shot_result_page_widgets() {
    shot_display();

    let page = ShotResultPage::new();
    let imp = page.imp();
    assert!(!imp.toolbar.imp().targets.get_visible());
    assert!(!imp.toolbar.imp().finish.get_visible());
    assert!(!imp.toolbar.imp().history.get_visible());
    assert!(imp.toolbar.has_css_class("shot-result-toolbar"));
    assert!(
        imp.toasts
            .parent()
            .is_some_and(|parent| parent.is::<adw::ToolbarView>())
    );
    let mut toolbar_parent = imp.toolbar.parent();
    while let Some(parent) = toolbar_parent {
        assert!(!parent.is::<adw::ToastOverlay>());
        toolbar_parent = parent.parent();
    }
    assert_eq!(imp.toolbar.halign(), gtk4::Align::Center);
    assert!(!imp.menu.get_visible());
    assert!(imp.menu.is_primary());
    assert_eq!(imp.zoom.active_name().as_deref(), Some("fit"));
    assert!(imp.picture.can_shrink());
    assert_eq!(imp.picture.content_fit(), gtk4::ContentFit::ScaleDown);
    let mut ancestor = imp.picture.parent();
    let mut in_scroll = false;
    while let Some(widget) = ancestor {
        if widget.is::<gtk4::ScrolledWindow>() {
            in_scroll = true;
            break;
        }
        ancestor = widget.parent();
    }
    assert!(in_scroll, "the picture sits in a scrolled window");
    assert!(!imp.undo.is_sensitive() && !imp.redo.is_sensitive());
    let writes = Rc::new(Cell::new(0));
    imp.heading.connect_notify_local(Some("subtitle"), {
        let writes = Rc::clone(&writes);
        move |_, _| writes.set(writes.get() + 1)
    });
    page.set_size_text("1440 × 612");
    assert_eq!(imp.heading.subtitle(), "1440 × 612");
    let written = writes.get();
    page.set_size_text("1440 × 612");
    assert_eq!(writes.get(), written);

    page.set_name("Q&A <b>draft</b>");
    assert_eq!(page.name(), "Q&A <b>draft</b>");
    let name_writes = Rc::new(Cell::new(0));
    imp.name.connect_notify_local(Some("text"), {
        let name_writes = Rc::clone(&name_writes);
        move |_, _| name_writes.set(name_writes.get() + 1)
    });
    page.set_name("Q&A <b>draft</b>");
    assert_eq!(name_writes.get(), 0);
    let applied = Rc::new(Cell::new(0));
    page.connect_name_changed({
        let applied = Rc::clone(&applied);
        move |_| applied.set(applied.get() + 1)
    });
    imp.name.set_text("draft two");
    assert_eq!(page.name(), "Q&A <b>draft</b>");
    assert_eq!(applied.get(), 0);
    imp.name.emit_by_name::<()>("apply", &[]);
    assert_eq!(page.name(), "draft two");
    assert_eq!(applied.get(), 1);

    assert!(!imp.collision.get_visible());
    assert!(!imp.name.has_css_class("error"));
    page.set_collision(true);
    assert!(imp.collision.get_visible());
    assert_eq!(
        imp.collision.text(),
        "A file with this name exists. Saving adds “(2)”."
    );
    assert!(imp.name.has_css_class("error"));
    let collision_writes = Rc::new(Cell::new(0));
    imp.collision.connect_notify_local(Some("visible"), {
        let collision_writes = Rc::clone(&collision_writes);
        move |_, _| collision_writes.set(collision_writes.get() + 1)
    });
    page.set_collision(true);
    assert_eq!(collision_writes.get(), 0);
    page.set_collision(false);
    assert!(!imp.collision.get_visible());
    assert!(!imp.name.has_css_class("error"));

    let copies = Rc::new(Cell::new(0));
    let saves = Rc::new(Cell::new(0));
    let opens = Rc::new(Cell::new(0));
    let save_as = Rc::new(Cell::new(0));
    let news = Rc::new(Cell::new(0));
    page.connect_copy({
        let copies = Rc::clone(&copies);
        move |_| copies.set(copies.get() + 1)
    });
    page.connect_save({
        let saves = Rc::clone(&saves);
        move |_| saves.set(saves.get() + 1)
    });
    page.connect_open_with({
        let opens = Rc::clone(&opens);
        move |_| opens.set(opens.get() + 1)
    });
    page.connect_save_as({
        let save_as = Rc::clone(&save_as);
        move |_| save_as.set(save_as.get() + 1)
    });
    page.connect_new({
        let news = Rc::clone(&news);
        move |_| news.set(news.get() + 1)
    });
    imp.copy.emit_clicked();
    imp.actions.activate_action("save", None);
    imp.actions.activate_action("open-with", None);
    imp.actions.activate_action("save-as", None);
    imp.new_shot.emit_clicked();
    assert_eq!(
        (
            copies.get(),
            saves.get(),
            opens.get(),
            save_as.get(),
            news.get()
        ),
        (1, 1, 1, 1, 1)
    );

    let actions = Rc::new(RefCell::new(Vec::<String>::new()));
    imp.toolbar.connect_action({
        let actions = Rc::clone(&actions);
        move |_, action| actions.borrow_mut().push(action)
    });
    imp.undo.emit_clicked();
    imp.redo.emit_clicked();
    assert!(actions.borrow().is_empty(), "history starts insensitive");
    let undo_writes = Rc::new(Cell::new(0));
    imp.undo.connect_notify_local(Some("sensitive"), {
        let undo_writes = Rc::clone(&undo_writes);
        move |_, _| undo_writes.set(undo_writes.get() + 1)
    });
    page.set_can_undo(false);
    page.set_can_redo(false);
    assert_eq!(undo_writes.get(), 0);
    page.set_can_undo(true);
    page.set_can_redo(true);
    assert_eq!(undo_writes.get(), 1);
    page.set_can_undo(true);
    assert_eq!(
        undo_writes.get(),
        1,
        "set_can_undo writes nothing when unchanged"
    );
    imp.undo.emit_clicked();
    imp.redo.emit_clicked();
    assert_eq!(actions.borrow().as_slice(), ["undo", "redo"]);

    let zoom_writes = Rc::new(Cell::new(0));
    imp.picture.connect_notify_local(Some("can-shrink"), {
        let zoom_writes = Rc::clone(&zoom_writes);
        move |_, _| zoom_writes.set(zoom_writes.get() + 1)
    });
    page.set_zoom("fit");
    page.set_zoom("nope");
    assert_eq!(zoom_writes.get(), 0);
    page.set_zoom("actual");
    assert_eq!(zoom_writes.get(), 1);
    assert!(!imp.picture.can_shrink());
    assert!(!imp.picture.hexpands());
    assert_eq!(imp.picture.content_fit(), gtk4::ContentFit::ScaleDown);
    assert_eq!(imp.zoom.active_name().as_deref(), Some("actual"));
    page.set_zoom("actual");
    assert_eq!(zoom_writes.get(), 1);
    page.set_zoom("fit");
    assert!(imp.picture.can_shrink());
    assert_eq!(imp.picture.content_fit(), gtk4::ContentFit::ScaleDown);

    let menu_writes = Rc::new(Cell::new(0));
    imp.menu.connect_notify_local(Some("menu-model"), {
        let menu_writes = Rc::clone(&menu_writes);
        move |_, _| menu_writes.set(menu_writes.get() + 1)
    });
    let model = gtk4::gio::Menu::new();
    page.set_menu_model(Some(model.upcast_ref()));
    assert!(imp.menu.get_visible());
    assert_eq!(menu_writes.get(), 1);
    page.set_menu_model(Some(model.upcast_ref()));
    assert_eq!(menu_writes.get(), 1);
    page.set_menu_model(None);
    assert!(!imp.menu.get_visible());

    let toast = adw::Toast::new("Saved");
    toast.set_button_label(Some("Show in Folder"));
    page.add_toast(&toast);
    assert_eq!(toast.button_label().as_deref(), Some("Show in Folder"));

    assert_eq!(page.picture(), imp.picture.get());
    let renames = Rc::new(Cell::new(0));
    page.connect_name_changed({
        let renames = Rc::clone(&renames);
        move |_| renames.set(renames.get() + 1)
    });
    page.set_name("Q&A <b>draft</b>");
    assert_eq!(renames.get(), 0, "an unchanged name emits nothing");
    page.set_name("renamed");
    assert_eq!(page.name(), "renamed");
    assert_eq!(renames.get(), 0);
    imp.name.set_text("typed");
    assert_eq!(page.name(), "renamed");
    assert_eq!(renames.get(), 0);
    imp.name.emit_by_name::<()>("apply", &[]);
    assert_eq!(page.name(), "typed");
    assert_eq!(renames.get(), 1);
    assert_eq!(imp.heading.title(), "typed.png");
    page.set_name(&"é".repeat(300));
    assert_eq!(page.name().chars().count(), 200);

    let preferences = ShotPreferences::new();
    let fields = preferences.fields();
    assert!(fields.folder.activatable_widget().is_some());
    assert!(fields.keyboard.is_sensitive());
    for (button, color) in [
        (&fields.red, "red"),
        (&fields.yellow, "yellow"),
        (&fields.green, "green"),
        (&fields.blue, "blue"),
        (&fields.white, "white"),
    ] {
        assert!(button.has_css_class("shot-color") && button.has_css_class(color));
    }
    assert_eq!(fields.strokes.active_name().as_deref(), Some("medium"));
    let shortcuts =
        gtk4::Builder::from_resource("/me/aresa/GlimpseShell/widgets/shot_shortcuts.ui");
    assert!(
        shortcuts
            .object::<adw::ShortcutsDialog>("shortcuts")
            .is_some()
    );
}

#[test]
#[ignore = "needs a display"]
fn picture_sits_in_a_viewport_that_does_not_follow_focus() {
    shot_display();

    let page = ShotResultPage::new();
    let imp = page.imp();
    let viewport = imp
        .picture
        .parent()
        .and_downcast::<gtk4::Viewport>()
        .expect("the picture is inside an explicit viewport");
    assert!(!viewport.is_scroll_to_focus());
    assert_eq!(viewport.parent().as_ref(), Some(imp.scroller.upcast_ref()));
}
