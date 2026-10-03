use std::cell::RefCell;
use std::rc::Rc;

use super::*;

fn usage(id: &str, title: &str) -> Usage {
    Usage {
        id: id.to_owned(),
        icon: "google-chrome".into(),
        title: title.to_owned(),
        detail: Some("Camera \u{b7} Microphone".into()),
        stoppable: false,
    }
}

fn rows(parent: &gtk4::Box) -> Vec<Expandable> {
    let mut rows = Vec::new();
    let mut child = parent.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(row) = widget.downcast::<Expandable>() {
            rows.push(row);
        }
    }
    rows
}

fn head(holder: &Expandable) -> LegacyRow {
    holder.head::<LegacyRow>().expect("a head row")
}

#[test]
#[ignore = "needs a display"]
fn privacy_popover_states() {
    if gtk4::init().is_err() {
        return;
    }
    crate::register_resources().expect("resources");

    let popover = PrivacyPopover::new();
    let imp = popover.imp();
    assert!(!imp.usages.get_visible(), "nothing in use shows no section");
    assert!(!imp.mute.get_visible());

    popover.set_usages(&[usage("app:Chrome", "Google Chrome")]);
    assert!(imp.usages.get_visible());
    let chrome = rows(&imp.usage_rows);
    assert_eq!(chrome.len(), 1);
    assert_eq!(head(&chrome[0]).title().as_deref(), Some("Google Chrome"));
    assert!(head(&chrome[0]).subtitle().is_none(), "a use is one line");
    let status = |holder: &Expandable| {
        holder
            .details::<gtk4::Box>()
            .and_then(|card| card.first_child())
            .and_then(|facts| facts.first_child())
            .and_downcast::<LegacyRow>()
            .expect("a status line")
    };
    assert_eq!(
        status(&chrome[0]).value().as_deref(),
        Some("Camera \u{b7} Microphone"),
        "the card says what is in use"
    );
    assert!(
        head(&chrome[0]).activatable()
            && !status(&chrome[0]).activatable()
            && !chrome[0]
                .details::<gtk4::Box>()
                .and_then(|card| card.last_child())
                .is_some_and(|stop| stop.get_visible()),
        "a use with nothing to stop opens a card with no Stop sharing"
    );

    let renders_after_first = imp.renders.get();
    popover.set_usages(&[usage("app:Chrome", "Google Chrome")]);
    assert_eq!(
        imp.renders.get(),
        renders_after_first,
        "an unchanged usage list must never re-enter render_usages at all"
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
    let sharing = rows(&imp.usage_rows);
    assert_eq!(sharing[0], chrome[0], "the same app keeps its row");
    head(&sharing[0]).emit_by_name::<()>("clicked", &[]);
    assert!(
        sharing[0].expanded(),
        "a share that can be stopped opens its card"
    );
    let stop = sharing[0]
        .details::<gtk4::Box>()
        .and_then(|card| card.last_child())
        .and_downcast::<LegacyRow>()
        .expect("a Stop sharing row");
    assert_eq!(stop.title().as_deref(), Some("Stop sharing"));
    assert!(stop.has_css_class("row--destructive"));
    stop.emit_by_name::<()>("clicked", &[]);
    assert_eq!(*stopped.borrow(), ["app:Chrome".to_owned()]);

    popover.set_usages(&[usage("app:Chrome", "Google Chrome")]);
    assert!(
        !stop.get_visible(),
        "a share that ended takes Stop sharing with it"
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
    imp.mute.set_property("active", false);
    imp.mute.emit_by_name::<()>("clicked", &[]);
    assert_eq!(toggles.borrow().last(), Some(&true));
    popover.set_microphone_muted(None);
    assert!(!imp.mute.get_visible());

    popover.set_usages(&[]);
    assert!(!imp.usages.get_visible());
    assert!(imp.usage_rows.first_child().is_none());
}
