use super::*;

pub(super) fn check() {
    let strip = TrayStrip::new();
    let chip = |key: &str| TrayChip {
        key: key.to_owned(),
        spec: spec(key),
    };
    let shown_keys = |strip: &TrayStrip| -> Vec<String> {
        strip
            .imp()
            .shown
            .borrow()
            .iter()
            .map(|(key, _)| key.clone())
            .collect()
    };

    strip.set_items(&[chip("a"), chip("b"), chip("c")]);
    assert!(strip.is_visible());
    assert_eq!(shown_keys(&strip), ["a", "b", "c"]);
    assert!(
        strip.imp().hidden.borrow().is_empty(),
        "no cap means no overflow"
    );
    let chevron = strip.imp().chevron.borrow().clone().expect("chevron");
    assert!(!chevron.get_visible(), "a chevron over nothing is hidden");

    let first = strip.imp().shown.borrow()[0].1.clone();
    strip.set_items(&[chip("c"), chip("a"), chip("b")]);
    assert_eq!(shown_keys(&strip), ["c", "a", "b"]);
    assert_eq!(
        strip.imp().shown.borrow()[1].1,
        first,
        "a key keeps its widget across a reorder rather than rebuilding it"
    );

    strip.set_max_visible(2);
    assert_eq!(shown_keys(&strip), ["c", "a"]);
    assert_eq!(strip.imp().hidden.borrow().len(), 1);
    assert!(chevron.get_visible());

    let drawer = strip.imp().drawer.borrow().clone().expect("drawer");
    assert!(!drawer.reveals_child());
    chevron.set_active(true);
    assert!(drawer.reveals_child(), "the chevron opens the overflow");
    chevron.set_active(false);
    assert!(
        !drawer.reveals_child(),
        "the control that opens the overflow closes it"
    );

    chevron.set_active(true);
    strip.set_max_visible(0);
    assert!(
        !drawer.reveals_child() && !chevron.get_visible(),
        "an emptied overflow does not leave its drawer standing open on nothing"
    );

    let fired = Rc::new(RefCell::new(Vec::new()));
    strip.connect_activated({
        let fired = Rc::clone(&fired);
        move |_, key, button| fired.borrow_mut().push((key, button))
    });
    strip.emit_by_name::<()>("activated", &[&"c".to_owned(), &3u32]);
    assert_eq!(*fired.borrow(), [("c".to_owned(), 3u32)]);

    let scrolled = Rc::new(RefCell::new(Vec::new()));
    strip.connect_scrolled({
        let scrolled = Rc::clone(&scrolled);
        move |_, key, dx, dy| scrolled.borrow_mut().push((key, dx, dy))
    });
    strip.emit_by_name::<()>("scrolled", &[&"a".to_owned(), &0.0f64, &-1.0f64]);
    assert_eq!(*scrolled.borrow(), [("a".to_owned(), 0.0f64, -1.0f64)]);

    let order = |strip: &TrayStrip| -> Vec<String> {
        let mut names = Vec::new();
        let mut child = strip.first_child();
        while let Some(node) = child {
            names.push(node.type_().name().to_owned());
            child = node.next_sibling();
        }
        names
    };
    assert_eq!(
        order(&strip),
        ["GtkToggleButton", "GtkRevealer", "GtkBox"],
        "the chevron leads, so the overflow opens into the bar and not off its edge"
    );
    assert!(
        chevron.has_css_class("flat"),
        "the chevron keeps its frameless class, or the theme paints a button behind the icon"
    );
    assert_eq!(
        chevron.icon_name().as_deref(),
        Some("pan-start-symbolic"),
        "the chevron points back over the closed drawer"
    );
    strip.set_max_visible(2);
    chevron.set_active(true);
    assert!(
        chevron.has_css_class("tray-strip__chevron--open"),
        "the open drawer turns the chevron the way its chips travelled"
    );
    chevron.set_active(false);
    assert!(!chevron.has_css_class("tray-strip__chevron--open"));
    strip.set_max_visible(0);
    strip.set_overflow_edge(Edge::End);
    assert_eq!(
        order(&strip),
        ["GtkBox", "GtkRevealer", "GtkToggleButton"],
        "the other edge mirrors it, for an applet on the other side of the panel"
    );
    assert_eq!(
        chevron.icon_name().as_deref(),
        Some("pan-end-symbolic"),
        "and the chevron turns with it"
    );
    strip.set_orientation(gtk4::Orientation::Vertical);
    assert_eq!(chevron.icon_name().as_deref(), Some("pan-down-symbolic"));
    strip.set_orientation(gtk4::Orientation::Horizontal);
    strip.set_overflow_edge(Edge::Start);

    strip.set_items(&[]);
    assert!(!strip.is_visible(), "an empty strip reserves no bar space");
}
