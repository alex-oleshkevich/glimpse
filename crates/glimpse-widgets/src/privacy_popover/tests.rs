use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::Row;

fn usage(id: &str, title: &str) -> Usage {
    Usage {
        id: id.to_owned(),
        icon: "google-chrome".into(),
        title: title.to_owned(),
        detail: Some("Camera \u{b7} Microphone".into()),
        stoppable: false,
    }
}

fn rows<W: IsA<gtk4::Widget>>(parent: &gtk4::Box) -> Vec<W> {
    let mut rows = Vec::new();
    let mut child = parent.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(row) = widget.downcast::<W>() {
            rows.push(row);
        }
    }
    rows
}

fn head(item: &ExpanderRow) -> ActionRow {
    item.imp().head.get()
}

#[test]
#[ignore = "needs a display"]
fn privacy_popover_states() {
    gtk4::init().expect("display");
    crate::register_resources().expect("resources");

    let popover = PrivacyPopover::new();
    let imp = popover.imp();
    assert!(!imp.usages.get_visible(), "nothing in use shows no section");
    assert!(!imp.mute.get_visible());

    popover.set_usages(&[usage("app:Chrome", "Google Chrome")]);
    assert!(imp.usages.get_visible());
    let chrome = rows::<ExpanderRow>(&imp.usage_rows);
    assert_eq!(chrome.len(), 1);
    assert_eq!(chrome[0].title().as_deref(), Some("Google Chrome"));
    assert!(chrome[0].subtitle().is_none(), "a use is one line");
    let status = |item: &ExpanderRow| {
        item.details()
            .first_child()
            .and_then(|facts| facts.first_child())
            .and_downcast::<Row>()
            .expect("a status line")
    };
    assert_eq!(
        status(&chrome[0]).value().as_deref(),
        Some("Camera \u{b7} Microphone"),
        "the card says what is in use"
    );
    assert!(
        !chrome[0]
            .details()
            .last_child()
            .is_some_and(|stop| stop.get_visible()),
        "a use with nothing to stop opens a card with no Stop sharing"
    );

    popover.set_usages(&[usage("app:Chrome", "Google Chrome")]);
    assert_eq!(
        rows::<ExpanderRow>(&imp.usage_rows),
        chrome,
        "an unchanged usage list reuses its row rather than rebuilding it"
    );

    let stopped: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    popover.connect_stop_activated({
        let stopped = Rc::clone(&stopped);
        move |_, id| stopped.borrow_mut().push(id)
    });
    popover.set_usages(&[Usage {
        stoppable: true,
        ..usage("app:Chrome", "Google Chrome")
    }]);
    let sharing = rows::<ExpanderRow>(&imp.usage_rows);
    assert_eq!(sharing[0], chrome[0], "the same app keeps its row");
    head(&sharing[0]).activate();
    assert!(
        sharing[0].open(),
        "a share that can be stopped opens its card"
    );
    let stop = sharing[0]
        .details()
        .last_child()
        .and_downcast::<ActionRow>()
        .expect("a Stop sharing row");
    assert_eq!(
        stop.upcast_ref::<Row>().title().as_deref(),
        Some("Stop sharing")
    );
    assert_eq!(stop.upcast_ref::<Row>().tone(), RowTone::Destructive);
    stop.activate();
    assert_eq!(*stopped.borrow(), ["app:Chrome".to_owned()]);

    popover.set_usages(&[usage("app:Chrome", "Google Chrome")]);
    assert!(
        !stop.get_visible(),
        "a share that ended takes Stop sharing with it"
    );

    popover.set_usages(&[Usage {
        detail: None,
        ..usage("app:Chrome", "Google Chrome")
    }]);
    assert!(
        rows::<ExpanderRow>(&imp.usage_rows).is_empty(),
        "a use with nothing to say and nothing to stop opens nothing"
    );
    let plain = rows::<Row>(&imp.usage_rows);
    assert_eq!(plain.len(), 1);
    assert_eq!(plain[0].title().as_deref(), Some("Google Chrome"));
    popover.set_usages(&[usage("app:Chrome", "Google Chrome")]);
    assert!(
        rows::<Row>(&imp.usage_rows).is_empty() && rows::<ExpanderRow>(&imp.usage_rows).len() == 1,
        "a use that gains something to say gets its card back"
    );

    let toggles: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(Vec::new()));
    popover.connect_mute_toggled({
        let toggles = Rc::clone(&toggles);
        move |_, on| toggles.borrow_mut().push(on)
    });
    popover.set_microphone_muted(Some(true));
    assert!(imp.mute.get_visible() && imp.mute.active());
    assert!(
        toggles.borrow().is_empty(),
        "following the device's state is not a request to change it"
    );
    imp.mute.activate();
    assert_eq!(toggles.borrow().last(), Some(&false));
    popover.set_microphone_muted(None);
    assert!(!imp.mute.get_visible());

    popover.set_usages(&[]);
    assert!(!imp.usages.get_visible());
    assert!(imp.usage_rows.first_child().is_none());
}
