use super::*;

#[test]
#[ignore = "needs a display"]
fn shot_region_widgets() {
    shot_display();

    let region = ShotRegion::new();
    let imp = region.imp();
    assert!(!imp.size_label.get_visible());
    let writes = Rc::new(Cell::new(0));
    imp.size_label.connect_notify_local(Some("label"), {
        let writes = Rc::clone(&writes);
        move |_, _| writes.set(writes.get() + 1)
    });
    region.set_size_text("1440 × 612 & <b>");
    assert_eq!(imp.size_label.text(), "1440 × 612 & <b>");
    assert!(!imp.size_label.uses_markup());
    assert!(!imp.size_label.get_visible(), "no selection, no readout");
    let written = writes.get();
    region.set_size_text("1440 × 612 & <b>");
    assert_eq!(writes.get(), written);
    let redraws = Rc::new(Cell::new(0));
    imp.size_label.connect_notify_local(Some("visible"), {
        let redraws = Rc::clone(&redraws);
        move |_, _| redraws.set(redraws.get() + 1)
    });
    region.set_selection(Some((10.0, 20.0, 30.0, 40.0)));
    assert!(imp.size_label.get_visible());
    assert_eq!(redraws.get(), 1);
    region.set_selection(Some((10.0, 20.0, 30.0, 40.0)));
    assert_eq!(redraws.get(), 1, "an unchanged selection redraws nothing");
    region.set_selection(None);
    assert!(!imp.size_label.get_visible());
    assert_eq!(redraws.get(), 2);
    assert_eq!(
        imp.hint.text(),
        "Drag to select  ·  Space: whole screen  ·  Esc: cancel"
    );
    assert!(imp.hint.has_css_class("pill-surface") && imp.hint.has_css_class("pill-surface--pill"));
}
