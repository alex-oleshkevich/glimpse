use super::*;

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
