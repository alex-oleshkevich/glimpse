use super::*;

pub(super) fn geometry() {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let mut groups = Vec::new();
    for icon in [None, Some("folder-symbolic")] {
        let expander = ExpanderRow::new();
        expander
            .details()
            .append(&gtk4::Label::new(Some("Details")));
        let rows = [
            Row::new(),
            ActionRow::new().upcast::<Row>(),
            ToggleRow::new().upcast::<Row>(),
            expander.imp().head.get().upcast::<Row>(),
        ];
        for row in &rows {
            row.set_title(Some("Title"));
            row.set_icon_name(icon);
            row.set_value(Some("42"));
        }
        for row in &rows[..3] {
            column.append(row);
        }
        column.append(&expander);
        groups.push((icon, rows, expander));
    }

    let window = gtk4::Window::new();
    window.set_child(Some(&column));
    window.present();
    let context = glib::MainContext::default();
    while context.iteration(false) {}
    assert!(window.is_realized());
    let bounds = |widget: &gtk4::Widget| {
        let bounds = widget.compute_bounds(&column).expect("allocated row child");
        assert!(bounds.width() > 0.0);
        bounds
    };

    for open in [false, true, false] {
        for (_, _, expander) in &groups {
            expander.set_open(open);
            assert_eq!(expander.open(), open);
        }
        while context.iteration(false) {}
        let height = column.measure(gtk4::Orientation::Vertical, 480).1;
        column.allocate(480, height, -1, None);
        for (icon, rows, _) in &groups {
            let reference = rows[0].imp();
            let title_x = bounds(reference.title.upcast_ref()).x();
            let content = bounds(reference.content.upcast_ref());
            let right = content.x() + content.width();
            let icon_x = icon.map(|_| bounds(reference.icon.upcast_ref()).x());
            for (name, row) in ["Row", "ActionRow", "ToggleRow", "ExpanderRow head"]
                .into_iter()
                .zip(rows)
            {
                let title = bounds(row.imp().title.upcast_ref());
                assert_eq!(
                    title.x(),
                    title_x,
                    "{name} title x with icon {icon:?}, open {open}"
                );
                let content = bounds(row.imp().content.upcast_ref());
                assert_eq!(
                    content.x() + content.width(),
                    right,
                    "{name} content right edge with icon {icon:?}, open {open}"
                );
                if let Some(icon_x) = icon_x {
                    assert_eq!(
                        bounds(row.imp().icon.upcast_ref()).x(),
                        icon_x,
                        "{name} icon x, open {open}"
                    );
                }
            }
        }
    }
    window.close();
}

pub(super) fn check() {
    let row = ExpanderRow::new();
    let expandable = child_named::<Expandable>(&row, "expander-row__expandable");
    let head = child_named::<ActionRow>(&row, "expander-row__head");
    let details = child_named::<gtk4::Box>(&row, "expander-row__details");
    assert_eq!(expandable.head::<ActionRow>(), Some(head.clone()));
    assert_eq!(expandable.details::<gtk4::Box>(), Some(details.clone()));
    assert_eq!(head.accessible_role(), gtk4::AccessibleRole::Button);
    assert!(gtk4::test_accessible_has_state(
        &head,
        gtk4::AccessibleState::Expanded
    ));
    assert!(head.upcast_ref::<Row>().imp().chevron().get_visible());
    assert!(row.find_property(concat!("ex", "clusive")).is_none());
    assert!(expandable.find_property(concat!("ex", "clusive")).is_none());

    row.set_title(Some("Devices"));
    row.set_subtitle(Some("Connected"));
    row.set_tone(RowTone::Warning);
    assert_eq!(head.upcast_ref::<Row>().title().as_deref(), Some("Devices"));
    assert_eq!(
        head.upcast_ref::<Row>().subtitle().as_deref(),
        Some("Connected")
    );
    assert_eq!(head.upcast_ref::<Row>().tone(), RowTone::Warning);

    let toggles = Rc::new(RefCell::new(Vec::new()));
    row.connect_toggled({
        let toggles = Rc::clone(&toggles);
        move |_, open| toggles.borrow_mut().push(open)
    });
    head.emit_by_name::<()>("activated", &[]);
    assert!(row.open() && head.has_css_class("row--open"));
    assert!(gtk4::test_accessible_has_state(
        &head,
        gtk4::AccessibleState::Expanded
    ));
    head.emit_by_name::<()>("activated", &[]);
    assert!(!row.open());
    assert_eq!(*toggles.borrow(), [true, false]);

    let built = gtk4::Builder::from_string(
        r#"<interface><object class="ExpanderRow" id="row">
            <child type="end"><object class="GtkButton" id="end"/></child>
            <child type="details"><object class="GtkLabel" id="detail"/></child>
            <child><object class="GtkLabel" id="plain"/></child>
        </object></interface>"#,
    );
    let built_row: ExpanderRow = built.object("row").expect("ExpanderRow");
    let built_head = child_named::<ActionRow>(&built_row, "expander-row__head");
    let built_details = child_named::<gtk4::Box>(&built_row, "expander-row__details");
    let end: gtk4::Button = built.object("end").expect("end");
    assert_eq!(
        end.parent(),
        Some(built_head.upcast_ref::<Row>().imp().end().upcast())
    );
    assert_eq!(
        built_details.first_child(),
        built
            .object::<gtk4::Label>("detail")
            .map(|label| label.upcast())
    );
    assert_eq!(
        built_details.last_child(),
        built
            .object::<gtk4::Label>("plain")
            .map(|label| label.upcast())
    );

    let child = ToggleRow::new();
    child.upcast_ref::<Row>().set_inactive(true);
    details.append(&child);
    row.set_inactive(true);
    let window = gtk4::Window::new();
    window.set_child(Some(&row));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert!(head.is_sensitive() && head.has_css_class("row--interactive"));
    super::row_checks::click(
        &head,
        f64::from(head.width()) / 2.0,
        f64::from(head.height()) / 2.0,
    );
    assert!(row.open());
    assert!(child.upcast_ref::<Row>().inactive());
    head.activate();
    assert!(!row.open());
    row.set_inactive(false);
    assert!(child.upcast_ref::<Row>().inactive());
    window.close();
    row.set_tint(true);
    assert!(row.has_css_class("expander-row--tint") && row.has_css_class("row--warning"));
    row.set_title_lines(3);
    let title = &head.upcast_ref::<Row>().imp().title;
    assert!(title.wraps() && title.lines() == 3 && !title.is_single_line_mode());
    row.set_title_lines(9);
    assert_eq!(title.lines(), 4);
    row.set_title_lines(0);
    assert_eq!(title.lines(), 1);
    assert!(!title.wraps() && title.is_single_line_mode());

    let shell = PopoverShell::new();
    let body = shell.body();
    let first = ExpanderRow::new();
    let nested = ExpanderRow::new();
    child_named::<gtk4::Box>(&first, "expander-row__details").append(&nested);
    let second = ExpanderRow::new();
    let legacy = Expandable::new(&gtk4::Button::new());
    legacy.set_details(Some(&gtk4::Label::new(Some("legacy detail"))));
    let third = ExpanderRow::new();
    let outside = gtk4::Label::new(Some("outside"));
    body.append(&first);
    body.append(&second);
    body.append(&legacy);
    body.append(&third);
    body.append(&outside);
    let closed = Rc::new(Cell::new(0));
    let nested_closed = Rc::new(Cell::new(0));
    let third_closed = Rc::new(Cell::new(0));
    first.connect_toggled({
        let closed = Rc::clone(&closed);
        move |_, open| {
            if !open {
                closed.set(closed.get() + 1);
            }
        }
    });
    nested.connect_toggled({
        let closed = Rc::clone(&nested_closed);
        move |_, open| {
            if !open {
                closed.set(closed.get() + 1);
            }
        }
    });
    third.connect_toggled({
        let closed = Rc::clone(&third_closed);
        move |_, open| {
            if !open {
                closed.set(closed.get() + 1);
            }
        }
    });
    first.set_open(true);
    nested.set_open(true);
    assert!(first.open() && nested.open());
    second.set_open(true);
    assert!(!first.open() && !nested.open() && second.open());
    assert_eq!(closed.get(), 1);
    assert_eq!(nested_closed.get(), 1);
    third.set_open(true);
    assert!(!second.open() && third.open());
    legacy.set_expanded(true);
    assert!(!second.open() && legacy.expanded() && !third.open());
    for widget in [
        first.upcast_ref::<gtk4::Widget>(),
        second.upcast_ref::<gtk4::Widget>(),
        outside.upcast_ref::<gtk4::Widget>(),
    ] {
        assert!(widget.has_css_class(crate::drawer::RECEDED));
    }
    assert!(shell.dismiss_from(outside.upcast_ref()));
    assert!(!legacy.expanded() && !third.open());
    assert_eq!(third_closed.get(), 1);

    first.set_open(true);
    nested.set_open(true);
    first.set_open(false);
    assert!(
        !nested.open(),
        "closing a disclosure closes its nested drawer"
    );
    assert!(!outside.has_css_class(crate::drawer::RECEDED));
    assert!(!shell.dismiss_from(outside.upcast_ref()));

    let inner = Expandable::new(&gtk4::Button::new());
    inner.set_details(Some(&gtk4::Label::new(Some("nested detail"))));
    legacy.set_details(Some(&inner));
    legacy.set_expanded(true);
    inner.set_expanded(true);
    legacy.set_expanded(false);
    assert!(
        !inner.expanded(),
        "legacy disclosures close their nested drawers"
    );
    assert!(!outside.has_css_class(crate::drawer::RECEDED));
    assert!(!shell.dismiss_from(outside.upcast_ref()));
}
