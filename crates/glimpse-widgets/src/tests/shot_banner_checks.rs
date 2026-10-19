use super::*;

#[test]
#[ignore = "needs a display"]
fn shot_banner_widgets() {
    shot_display();

    let banner = ShotBanner::new();
    let imp = banner.imp();
    let writes = Rc::new(Cell::new(0));
    imp.body.connect_notify_local(Some("label"), {
        let writes = Rc::clone(&writes);
        move |_, _| writes.set(writes.get() + 1)
    });
    let prompt = format!("Circle & <b>{}", "é".repeat(520));
    banner.set_prompt(&prompt);
    assert_eq!(imp.body.text().chars().count(), 500);
    assert!(imp.body.text().starts_with("Circle & <b>"));
    assert!(!imp.body.uses_markup());
    let written = writes.get();
    banner.set_prompt(&prompt);
    assert_eq!(writes.get(), written);

    banner.set_portal_request("   ");
    assert!(imp.body.text().contains("An application"));
    assert!(imp.body.text().contains("asked for a screenshot"));
    assert!(!imp.body.text().contains("Enter shares"));
    assert!(imp.keys.get_visible());
    assert_eq!(imp.keys.text(), "Enter shares · Esc refuses");
    assert!(imp.keys.has_css_class("shot-banner__keys"));
    banner.set_portal_request("GIMP & <b>");
    assert!(imp.body.text().contains("GIMP & <b>"));
    assert!(!imp.body.uses_markup());
    let written = writes.get();
    banner.set_portal_request("GIMP & <b>");
    assert_eq!(
        writes.get(),
        written,
        "the same portal request writes nothing"
    );
    banner.set_portal_request(&"é".repeat(250));
    assert_eq!(imp.body.text().matches('é').count(), 200);
    assert!(imp.body.text().contains('…'));
    banner.set_prompt("done");
    assert!(!imp.keys.get_visible());
    assert_eq!(imp.body.text(), "done");
}
