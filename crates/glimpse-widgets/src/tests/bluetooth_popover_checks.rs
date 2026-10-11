use super::*;

pub(super) fn check() {
    let popover = BluetoothPopover::new();
    let powered = Rc::new(RefCell::new(Vec::new()));
    popover.connect_powered({
        let powered = Rc::clone(&powered);
        move |_, on| powered.borrow_mut().push(on)
    });
    popover.set_adapter("Bluetooth", true, true);
    assert!(popover.imp().power.is_active());
    assert!(popover.imp().shell.subtitle().is_none());
    assert!(powered.borrow().is_empty());
    popover.imp().power.set_active(false);
    assert_eq!(*powered.borrow(), [false]);
    popover.set_adapter("Bluetooth", true, true);
    assert_eq!(*powered.borrow(), [false]);
    assert!(!popover.imp().devices.get_visible());

    let entry = |id: &str, place: BluetoothPlace| BluetoothEntry {
        id: id.to_owned(),
        title: id.to_owned(),
        problem: String::new(),
        icon: "audio-headset-symbolic".to_owned(),
        place,
        busy: false,
    };
    popover.set_entries(&[
        entry("a", BluetoothPlace::Connected),
        entry("b", BluetoothPlace::Paired),
        entry("c", BluetoothPlace::Paired),
    ]);
    assert!(popover.imp().devices.get_visible());
    assert_eq!(popover.imp().devices_held.borrow().len(), 3);

    let device = |id: &str| -> crate::bluetooth_popover::BluetoothDevice {
        let imp = popover.imp();
        for held in [&imp.devices_held, &imp.nearby_held] {
            if let Some((_, device)) = held.borrow().iter().find(|(key, _)| key == id) {
                return device.clone();
            }
        }
        panic!("no row is holding {id}");
    };
    let row = |id: &str| device(id).imp().row.get();
    let press = |row: &ExpanderRow| row.imp().head.activate();
    let dimmed = |widget: &gtk4::Widget| {
        std::iter::successors(Some(widget.clone()), |widget| widget.parent())
            .any(|widget| widget.has_css_class("receded"))
    };
    let first = device("a");
    assert!(first.imp().status.get_visible());
    assert!(first.imp().status.has_css_class("accent"));
    assert_eq!(
        first.imp().status.icon_name().as_deref(),
        Some("object-select-symbolic")
    );
    assert!(!device("b").imp().status.get_visible());
    assert!(row("a").value().is_none());
    let end = first.imp().status.parent().expect("row end slot");
    assert!(end.get_visible());
    let mut busy = entry("a", BluetoothPlace::Connected);
    busy.busy = true;
    first.set_entry(&busy);
    assert!(!end.get_visible());
    popover.set_entries(&[
        entry("a", BluetoothPlace::Paired),
        entry("b", BluetoothPlace::Paired),
    ]);
    assert!(!first.imp().status.get_visible());
    popover.set_entries(&[
        entry("a", BluetoothPlace::Connected),
        entry("b", BluetoothPlace::Paired),
    ]);
    assert_eq!(device("a"), first, "a stable key keeps its drawer");

    let acted = Rc::new(RefCell::new(Vec::new()));
    popover.connect_acted({
        let acted = Rc::clone(&acted);
        move |_, id, action| acted.borrow_mut().push(format!("{id}/{action}"))
    });
    let toggled = Rc::new(RefCell::new(Vec::new()));
    popover.connect_toggled({
        let toggled = Rc::clone(&toggled);
        move |_, id, action, on| toggled.borrow_mut().push(format!("{id}/{action}/{on}"))
    });

    let primary = |action: &str, title: &str| BluetoothLine {
        action: action.to_owned(),
        title: title.to_owned(),
        activates: true,
        ..Default::default()
    };
    let trust = |on: bool| BluetoothLine {
        action: "trust".to_owned(),
        title: "Connect automatically".to_owned(),
        toggle: Some(on),
        ..Default::default()
    };
    let forget = BluetoothLine {
        destructive: true,
        ..primary("forget", "Forget this device")
    };
    let in_use = |on: bool| BluetoothDetails {
        id: "a".to_owned(),
        facts: vec![Fact::new("Battery", "72%")],
        lines: vec![
            primary("disconnect", "Disconnect"),
            trust(on),
            forget.clone(),
        ],
    };
    let paired = BluetoothDetails {
        id: "b".to_owned(),
        facts: vec![Fact::new("Type", "Headset")],
        lines: vec![primary("connect", "Connect"), trust(true), forget.clone()],
    };
    popover.set_details(&[in_use(false), paired.clone()]);
    assert!(!row("a").open(), "data updates open nothing");

    press(&row("a"));
    assert!(row("a").open());
    assert!(acted.borrow().is_empty());
    press(&row("a"));
    assert!(!row("a").open(), "the second press closes the drawer");
    press(&row("a"));
    assert!(
        !dimmed(row("a").upcast_ref())
            && dimmed(row("b").upcast_ref())
            && dimmed(popover.imp().shell.imp().header.upcast_ref()),
        "the device stays lit while the surrounding content recedes"
    );

    let a = device("a");
    let lines = children_of::<Row>(&row("a").details());
    assert_eq!(lines.len(), 3);
    assert!(lines[0].is::<ToggleRow>());
    assert!(lines[1].is::<ActionRow>());
    assert!(lines[2].is::<ActionRow>());
    assert_eq!(lines[1].title().as_deref(), Some("Disconnect"));
    assert_eq!(lines[2].tone(), RowTone::Destructive);
    assert_eq!(
        row("a").details().first_child(),
        Some(a.imp().facts.get().upcast()),
        "the facts lead the drawer"
    );
    a.imp().trust.emit_by_name::<()>("activate", &[]);
    a.imp().trust.emit_by_name::<()>("activate", &[]);
    assert_eq!(*toggled.borrow(), ["a/trust/true", "a/trust/false"]);
    assert!(!a.imp().trust.active());
    assert!(
        children_of::<Row>(&*a.imp().facts)
            .iter()
            .all(|row| row.type_() == Row::static_type())
    );
    let facts = children_of::<Row>(&*a.imp().facts);
    assert_eq!(facts[0].title().as_deref(), Some("Battery"));
    assert_eq!(facts[0].value().as_deref(), Some("72%"));
    a.imp().primary.activate();
    assert_eq!(*acted.borrow(), ["a/disconnect"]);
    assert!(
        row("a").open(),
        "dispatch waits for a successful reply to close"
    );

    popover.set_details(&[in_use(true), paired.clone()]);
    assert!(a.imp().trust.active());
    assert_eq!(*toggled.borrow(), ["a/trust/true", "a/trust/false"]);
    assert_eq!(*acted.borrow(), ["a/disconnect"]);
    a.imp().trust.emit_by_name::<()>("activate", &[]);
    assert!(!a.imp().trust.active());
    popover.set_details(&[in_use(true), paired.clone()]);
    assert!(
        a.imp().trust.active(),
        "unchanged details restore a toggle the command did not move"
    );

    press(&row("b"));
    assert!(!row("a").open() && row("b").open());
    assert_eq!(
        device("b")
            .imp()
            .primary
            .upcast_ref::<Row>()
            .title()
            .as_deref(),
        Some("Connect")
    );
    device("b").imp().primary.activate();
    device("b").imp().forget.activate();
    assert_eq!(*acted.borrow(), ["a/disconnect", "b/connect", "b/forget"]);
    popover.collapse("b");
    assert!(!row("b").open());
    assert!(row("b").subtitle().is_none());

    let pressed = Rc::new(Cell::new(0u32));
    popover.connect_nearby_toggled({
        let pressed = Rc::clone(&pressed);
        move |_| pressed.set(pressed.get() + 1)
    });
    popover.set_nearby(0, true);
    assert!(!popover.imp().nearby.get_visible() && !popover.imp().search.open());
    popover.set_entries(&[
        entry("a", BluetoothPlace::Connected),
        entry("b", BluetoothPlace::Paired),
        entry("n", BluetoothPlace::Nearby),
    ]);
    let nearby = BluetoothDetails {
        id: "n".to_owned(),
        facts: vec![Fact::new("Type", "Headset")],
        lines: vec![primary("pair", "Pair")],
    };
    popover.set_details(&[in_use(true), paired.clone(), nearby]);
    popover.set_nearby(1, false);
    assert!(popover.imp().nearby.get_visible() && !popover.imp().search.open());
    popover.set_nearby(1, true);
    assert!(popover.imp().search.open());
    assert_eq!(pressed.get(), 0, "backend updates never dispatch a toggle");
    press(&popover.imp().search);
    assert!(!popover.imp().search.open());
    press(&popover.imp().search);
    assert!(popover.imp().search.open());
    assert_eq!(pressed.get(), 2, "each disclosure press dispatches once");
    press(&row("n"));
    assert!(
        popover.imp().search.open() && row("n").open(),
        "opening a nearby device preserves its disclosure ancestor"
    );
    let n = device("n");
    assert_eq!(
        n.imp().primary.upcast_ref::<Row>().title().as_deref(),
        Some("Pair")
    );
    assert!(!n.imp().trust.get_visible() && !n.imp().forget.get_visible());
    n.imp().primary.activate();
    assert_eq!(acted.borrow().last().map(String::as_str), Some("n/pair"));
    popover.collapse("n");
    assert!(popover.imp().search.open() && !row("n").open());

    let expanded = Rc::new(RefCell::new(Vec::new()));
    popover.connect_expanded({
        let expanded = Rc::clone(&expanded);
        move |_, place| expanded.borrow_mut().push(place.to_owned())
    });
    popover.set_overflow(Some("Show more"), Some("More nearby"));
    popover.imp().more_paired.activate();
    popover.imp().more_nearby.activate();
    assert_eq!(*expanded.borrow(), ["paired", "nearby"]);
    popover.set_overflow(None, None);
    assert!(!popover.imp().more_paired.get_visible() && !popover.imp().more_nearby.get_visible());

    popover.set_controls_sensitive(false);
    assert!(!popover.imp().search.get_sensitive());
    popover.set_controls_sensitive(true);

    let width = |popover: &BluetoothPopover| popover.measure(gtk4::Orientation::Horizontal, -1).1;
    let probe = BluetoothPopover::new();
    probe.set_adapter("Bluetooth", true, true);
    probe.set_entries(&[entry("a", BluetoothPlace::Connected)]);
    let floor = width(&probe);
    probe.set_adapter(
        "Connected to Bose QuietComfort Ultra and 2 others",
        true,
        true,
    );
    assert_eq!(
        width(&probe),
        floor,
        "a growing heading keeps the card's width"
    );
    probe.set_details(&[BluetoothDetails {
        id: "a".to_owned(),
        facts: vec![Fact::new(
            "Codec",
            "Audio, Calls, Remote control, Network, File transfer",
        )],
        lines: vec![primary("disconnect", "Disconnect")],
    }]);
    let opened = probe
        .imp()
        .devices_held
        .borrow()
        .first()
        .map(|(_, device)| device.imp().row.get())
        .expect("the device");
    press(&opened);
    assert!(opened.open());
    assert_eq!(width(&probe), floor, "long facts keep the card's width");

    let answers = Rc::new(RefCell::new(Vec::new()));
    probe.connect_answered({
        let answers = Rc::clone(&answers);
        move |_, accepted| answers.borrow_mut().push(accepted)
    });
    probe.set_prompt(Some(&BluetoothAsk {
        key: "pair:/org/bluez/hci0/dev_a".to_owned(),
        device: "Pixel 9 Pro".to_owned(),
        question: "Is this the code shown on the device?".to_owned(),
        code: "419 274".to_owned(),
        progress: String::new(),
        accept: "Confirm".to_owned(),
        cancel: "Cancel".to_owned(),
        destructive: false,
    }));
    assert!(!opened.open(), "a prompt closes the device drawer");
    assert_eq!(width(&probe), floor, "a prompt preserves the surface width");
    assert_eq!(
        probe.imp().pages.visible_child_name().as_deref(),
        Some("prompt")
    );
    assert!(!probe.imp().shell.imp().header.get_sensitive());
    assert_eq!(probe.imp().prompt_device.text(), "Pixel 9 Pro");
    assert_eq!(
        probe.imp().prompt_ask.text(),
        "Is this the code shown on the device?"
    );
    assert_eq!(probe.imp().prompt_code.text(), "419 274");
    assert!(probe.imp().prompt_accept.get_visible() && probe.imp().prompt_code.get_visible());
    probe.imp().prompt_accept.emit_by_name::<()>("clicked", &[]);
    probe.imp().prompt_cancel.emit_by_name::<()>("clicked", &[]);
    assert_eq!(*answers.borrow(), [true, false]);

    probe.set_prompt(Some(&BluetoothAsk {
        key: "pair:/org/bluez/hci0/dev_long".to_owned(),
        device: "HP OfficeJet 8025".to_owned(),
        question: "Type this PIN on the device.".to_owned(),
        code: "0".repeat(32),
        progress: String::new(),
        accept: String::new(),
        cancel: "Cancel".to_owned(),
        destructive: false,
    }));
    assert_eq!(width(&probe), floor, "a peer's code cannot widen the card");
    assert!(!probe.imp().prompt_accept.get_visible());
    probe.set_prompt(Some(&BluetoothAsk {
        key: "forget:/org/bluez/hci0/dev_b".to_owned(),
        device: "UE BOOM 3".to_owned(),
        question: "Forget this device?".to_owned(),
        code: String::new(),
        progress: String::new(),
        accept: "Forget".to_owned(),
        cancel: "Cancel".to_owned(),
        destructive: true,
    }));
    assert!(
        probe
            .imp()
            .prompt_accept
            .has_css_class("prompt__accept--danger")
    );
    assert!(!probe.imp().prompt_code.get_visible());
    assert!(
        probe
            .imp()
            .prompt_actions
            .has_css_class("prompt__actions--bare")
    );
    probe.set_prompt(None);
    assert_eq!(
        probe.imp().pages.visible_child_name().as_deref(),
        Some("devices")
    );
    assert!(probe.imp().shell.imp().header.get_sensitive());
    probe.set_nearby(1, true);
    probe.set_prompt(Some(&BluetoothAsk {
        key: "pair:/org/bluez/hci0/dev_n".to_owned(),
        ..Default::default()
    }));
    assert!(!probe.imp().search.open());
    probe.set_nearby(1, true);
    assert!(
        !probe.imp().search.open(),
        "a hidden disclosure cannot dim the prompt"
    );
}
