use super::*;

pub(super) fn check() {
    let row = SliderRow::new();
    let scale = child_named::<gtk4::Scale>(&row, "slider-row__scale");
    let start = child_named::<gtk4::Box>(&row, "slider-row__start");
    assert!(!start.get_visible());
    let button = gtk4::Button::new();
    row.set_start(&button);
    assert_eq!(button.parent().as_ref(), Some(start.upcast_ref()));
    assert!(start.get_visible());
    assert_eq!(
        (
            row.property::<f64>("min"),
            row.property::<f64>("max"),
            row.step()
        ),
        (0.0, 1.0, 0.01)
    );
    row.set_title(Some("Volume"));
    assert!(gtk4::test_accessible_has_property(
        &scale,
        gtk4::AccessibleProperty::Label
    ));
    row.set_title(Some("Output volume"));
    assert_eq!(row.title().as_deref(), Some("Output volume"));

    row.set_inactive(true);
    assert!(!scale.is_sensitive());
    assert!(button.is_sensitive());
    row.set_inactive(false);
    assert!(scale.is_sensitive());

    let moved = Rc::new(RefCell::new(Vec::new()));
    let changed = Rc::new(RefCell::new(Vec::new()));
    row.connect_moved({
        let moved = Rc::clone(&moved);
        move |_, value| moved.borrow_mut().push(value)
    });
    row.connect_changed({
        let changed = Rc::clone(&changed);
        move |_, value| changed.borrow_mut().push(value)
    });
    row.set_value(0.2);
    row.imp().held.set(true);
    scale.set_value(0.4);
    row.set_value(0.9);
    assert_eq!(row.value(), 0.4);
    row.imp().finish_drag();
    assert_eq!(row.value(), 0.4);
    assert_eq!(*moved.borrow(), [0.4]);
    assert_eq!(*changed.borrow(), [0.4]);

    row.set_value(0.9);
    assert_eq!(row.value(), 0.9);
    row.imp().held.set(true);
    row.set_value(0.1);
    scale.set_value(0.7);
    row.imp().finish_drag();
    assert_eq!(row.value(), 0.7);
    assert_eq!(*changed.borrow(), [0.4, 0.7]);
    row.set_value(f64::NAN);
    row.set_value(f64::INFINITY);
    assert_eq!(row.value(), 0.7);
    row.set_value(2.0);
    assert_eq!(row.value(), 1.0);
    row.set_min(2.0);
    assert_eq!(
        (
            row.property::<f64>("min"),
            row.property::<f64>("max"),
            row.value()
        ),
        (2.0, 2.0, 2.0)
    );
    row.set_max(1.0);
    assert_eq!(row.property::<f64>("max"), 2.0);
    row.set_min(0.0);
    row.set_max(1.0);
    let _: bool = scale.emit_by_name("change-value", &[&gtk4::ScrollType::StepForward, &0.5f64]);
    assert_eq!(changed.borrow().last(), Some(&0.5));

    restrictions(&row, &scale, &button, &changed, &moved);
}

fn restrictions(
    row: &SliderRow,
    scale: &gtk4::Scale,
    button: &gtk4::Button,
    changed: &Rc<RefCell<Vec<f64>>>,
    moved: &Rc<RefCell<Vec<f64>>>,
) {
    row.set_subtitle(Some("Muted"));
    row.set_value(0.6);
    row.imp().held.set(true);
    scale.set_value(0.7);
    changed.borrow_mut().clear();
    moved.borrow_mut().clear();
    row.set_inactive(true);
    row.imp().finish_drag();
    assert!(changed.borrow().is_empty());
    assert!(!scale.is_sensitive() && button.is_sensitive());
    assert!(child_named::<gtk4::Label>(row, "slider-row__subtitle").get_visible());
    row.set_value(0.8);
    assert_eq!(row.value(), 0.8);
    scale.emit_by_name::<bool>("change-value", &[&gtk4::ScrollType::StepForward, &0.9f64]);
    scale.set_value(0.9);
    assert_eq!(row.value(), 0.8);
    assert!(changed.borrow().is_empty() && moved.borrow().is_empty());
    row.set_inactive(false);
    assert!(scale.is_sensitive() && scale.is_focusable() && button.is_sensitive());
    row.set_subtitle(Some("я".repeat(10_000)));
    assert_eq!(row.subtitle().unwrap().chars().count(), TEXT_MAX_CHARS);
    row.set_subtitle(Some(""));
    assert_eq!(row.subtitle(), None);
    assert!(!child_named::<gtk4::Label>(row, "slider-row__subtitle").get_visible());
}
