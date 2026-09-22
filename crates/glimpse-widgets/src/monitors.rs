use std::rc::Rc;

use gtk4::{gdk, gio, prelude::*};

pub fn watch_monitors(changed: impl Fn() + 'static) {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let changed: Rc<dyn Fn()> = Rc::new(changed);
    let model = display.monitors();
    follow_connectors(&model, 0, model.n_items(), &changed);
    model.connect_items_changed(move |model, position, _, added| {
        follow_connectors(model, position, added, &changed);
        changed();
    });
}

fn follow_connectors(model: &gio::ListModel, position: u32, count: u32, changed: &Rc<dyn Fn()>) {
    for index in position..position + count {
        let Some(monitor) = model.item(index).and_downcast::<gdk::Monitor>() else {
            continue;
        };
        let changed = changed.clone();
        monitor.connect_connector_notify(move |_| changed());
    }
}
