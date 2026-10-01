use super::*;

pub(super) fn check() {
    let swatch = Swatch::default();
    let swatch_notified = Rc::new(Cell::new(0));
    swatch.connect_color_notify({
        let swatch_notified = Rc::clone(&swatch_notified);
        move |_| swatch_notified.set(swatch_notified.get() + 1)
    });
    swatch.set_color(Some(&rgba([224, 86, 63])));
    swatch.set_color(Some(&rgba([224, 86, 63])));
    assert_eq!(swatch_notified.get(), 1);
    assert_eq!(swatch.color(), Some(rgba([224, 86, 63])));

    let chip = Indicator::new();
    let extension = Swatch::default();
    let spec = IndicatorSpec {
        extension: Some(extension.clone().upcast()),
        ..Default::default()
    };
    chip.apply(&spec);
    chip.apply(&spec);
    assert!(chip.imp().extension.get_visible());
    assert_eq!(
        chip.imp().extension.first_child(),
        Some(extension.clone().upcast())
    );
    assert!(
        chip.imp()
            .extension
            .first_child()
            .unwrap()
            .next_sibling()
            .is_none()
    );
    assert!(!chip.imp().icon_slot.get_visible());
    chip.apply(&IndicatorSpec::default());
    assert!(!chip.imp().extension.get_visible());
    assert!(chip.imp().extension.first_child().is_none());
    assert!(extension.parent().is_none());
}
