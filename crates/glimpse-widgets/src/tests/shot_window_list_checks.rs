use super::*;

#[test]
#[ignore = "needs a display"]
fn shot_window_list_widgets() {
    if !shot_display() {
        return;
    }

    let list = ShotWindowList::new();
    let imp = list.imp();
    assert!(!imp.this_section.get_visible() && !imp.other_section.get_visible());

    let title = format!("Q&A <b>draft</b>{}", "é".repeat(190));
    let here = ShotWindowEntry {
        id: 7,
        app_id: Some("org.gnome.TextEditor".to_owned()),
        title: title.clone(),
        name: None,
        icon: None,
        this_workspace: true,
        focused: true,
        captured_when_picked: false,
    };
    let elsewhere = ShotWindowEntry {
        id: 8,
        app_id: Some("app & <b>".to_owned()),
        title: "other".to_owned(),
        name: None,
        icon: None,
        this_workspace: false,
        focused: false,
        captured_when_picked: true,
    };
    list.set_windows(&[here.clone(), elsewhere.clone()]);
    assert!(imp.this_section.get_visible() && imp.other_section.get_visible());

    let current = imp.this_rows.first_child().expect("the focused row");
    assert!(current.has_css_class("shot-list__row--current"));
    let title_label = child_named::<gtk4::Label>(&current, "shot-list__title");
    assert_eq!(title_label.text().chars().count(), 200);
    assert!(title_label.text().contains("Q&A <b>draft</b>"));
    assert!(!title_label.uses_markup());
    assert!(
        !title_label.get_visible(),
        "the title is already the first line"
    );
    let note = child_named::<gtk4::Label>(&current, "shot-list__note");
    assert!(!note.get_visible());

    let other = imp.other_rows.first_child().expect("the other row");
    assert!(!other.has_css_class("shot-list__row--current"));
    let other_note = child_named::<gtk4::Label>(&other, "shot-list__note");
    assert!(other_note.get_visible());
    assert_eq!(other_note.text(), "captured when picked");
    let primary = other
        .first_child()
        .and_then(|row| row.first_child())
        .and_then(|icon| icon.next_sibling())
        .and_then(|text| text.first_child())
        .and_downcast::<gtk4::Label>()
        .expect("the app id label");
    assert_eq!(primary.text(), "other");
    assert!(!primary.uses_markup());
    assert!(!child_named::<gtk4::Label>(&other, "shot-list__title").get_visible());

    let picked = Rc::new(RefCell::new(Vec::<u64>::new()));
    list.connect_window_picked({
        let picked = Rc::clone(&picked);
        move |_, id| picked.borrow_mut().push(id)
    });
    current
        .downcast_ref::<gtk4::Button>()
        .expect("a row button")
        .emit_clicked();
    assert_eq!(picked.borrow().as_slice(), [7]);

    let kept = imp.this_rows.first_child();
    list.set_windows(&[here, elsewhere]);
    assert_eq!(
        imp.this_rows.first_child(),
        kept,
        "an unchanged list rebuilds nothing"
    );

    list.set_windows(&[ShotWindowEntry {
        id: 1,
        app_id: None,
        title: "only elsewhere".to_owned(),
        name: None,
        icon: None,
        this_workspace: false,
        focused: true,
        captured_when_picked: false,
    }]);
    assert!(!imp.this_section.get_visible());
    assert!(imp.other_section.get_visible());
    list.set_windows(&[]);
    assert!(!imp.this_section.get_visible() && !imp.other_section.get_visible());

    let name = format!("N& <b>{}", "é".repeat(210));
    list.set_windows(&[ShotWindowEntry {
        id: 3,
        app_id: Some("org.example.Unused".to_owned()),
        title: "notes & <b>".to_owned(),
        name: Some(name),
        icon: Some("edit-copy-symbolic".to_owned()),
        this_workspace: true,
        focused: false,
        captured_when_picked: false,
    }]);
    let named = imp.this_rows.first_child().expect("the named row");
    let named_primary = named
        .first_child()
        .and_then(|row| row.first_child())
        .and_then(|icon| icon.next_sibling())
        .and_then(|text| text.first_child())
        .and_downcast::<gtk4::Label>()
        .expect("the name label");
    assert_eq!(named_primary.text().chars().count(), 200);
    assert!(named_primary.text().starts_with("N& <b>"));
    assert!(!named_primary.uses_markup());
    let named_title = child_named::<gtk4::Label>(&named, "shot-list__title");
    assert!(named_title.get_visible());
    assert_eq!(named_title.text(), "notes & <b>");
    let named_icon = named
        .first_child()
        .and_then(|row| row.first_child())
        .and_downcast::<gtk4::Image>()
        .expect("the icon");
    assert_eq!(
        named_icon.icon_name().as_deref(),
        Some("edit-copy-symbolic")
    );

    list.set_windows(&[ShotWindowEntry {
        id: 4,
        app_id: Some("no-such-icon".to_owned()),
        title: String::new(),
        name: None,
        icon: None,
        this_workspace: true,
        focused: false,
        captured_when_picked: false,
    }]);
    let fallback = imp.this_rows.first_child().expect("the fallback row");
    let fallback_primary = fallback
        .first_child()
        .and_then(|row| row.first_child())
        .and_then(|icon| icon.next_sibling())
        .and_then(|text| text.first_child())
        .and_downcast::<gtk4::Label>()
        .expect("the app id label");
    assert_eq!(fallback_primary.text(), "no-such-icon");
    assert!(!child_named::<gtk4::Label>(&fallback, "shot-list__title").get_visible());
    let fallback_icon = fallback
        .first_child()
        .and_then(|row| row.first_child())
        .and_downcast::<gtk4::Image>()
        .expect("the fallback icon");
    assert_eq!(fallback_icon.icon_name().as_deref(), Some("no-such-icon"));

    let window = gtk4::Window::new();
    window.set_default_size(320, 200);
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let stay = gtk4::Button::with_label("stay");
    column.append(&stay);
    column.append(&list);
    window.set_child(Some(&column));
    window.set_visible(true);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(2000);
    while !list.is_mapped() && std::time::Instant::now() < deadline {
        if !gtk4::glib::MainContext::default().iteration(false) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    assert!(list.is_mapped(), "the list is mapped");
    assert!(stay.grab_focus());
    assert!(stay.is_focus());
    list.set_windows(&[ShotWindowEntry {
        id: 9,
        app_id: None,
        title: "focused".to_owned(),
        name: None,
        icon: None,
        this_workspace: true,
        focused: true,
        captured_when_picked: false,
    }]);
    assert!(
        stay.is_focus(),
        "set_windows does not move focus while mapped"
    );
    let current = imp.this_rows.first_child().expect("the current row");
    assert!(current.has_css_class("shot-list__row--current"));
    assert!(!current.is_focus());
    window.close();
}
