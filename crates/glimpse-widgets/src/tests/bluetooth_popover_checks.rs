use super::*;

pub(super) fn check() {
    let popover = BluetoothPopover::new();
    popover.set_adapter(
        "Bluetooth",
        "No device connected",
        "bluetooth-symbolic",
        true,
        true,
    );
    assert!(popover.imp().power.is_active());
    assert!(!popover.imp().devices.get_visible());

    let entry = |id: &str, place: BluetoothPlace| BluetoothEntry {
        id: id.to_owned(),
        title: id.to_owned(),
        problem: String::new(),
        icon: "audio-headset-symbolic".to_owned(),
        place,
        value: String::new(),
        busy: false,
    };
    popover.set_entries(&[
        entry("a", BluetoothPlace::Connected),
        entry("b", BluetoothPlace::Paired),
        entry("c", BluetoothPlace::Paired),
    ]);
    assert!(popover.imp().devices.get_visible());
    assert_eq!(
        popover.imp().devices_held.borrow().len(),
        3,
        "connected and paired devices are one list"
    );

    let holder = |id: &str| -> Expandable {
        let imp = popover.imp();
        for held in [&imp.devices_held, &imp.nearby_held] {
            if let Some((_, holder)) = held.borrow().iter().find(|(key, _)| key == id) {
                return holder.clone();
            }
        }
        panic!("no row is holding {id}");
    };
    let card_of = |id: &str| -> gtk4::Box {
        holder(id)
            .details::<gtk4::Box>()
            .expect("an opened device has a card")
    };
    let rows_of = |id: &str| -> Vec<LegacyRow> {
        children_of::<LegacyRow>(
            &card_of(id)
                .last_child()
                .and_downcast::<gtk4::Box>()
                .expect("a card ends in its lines"),
        )
    };
    let dimmed = |widget: &gtk4::Widget| {
        std::iter::successors(Some(widget.clone()), |widget| widget.parent())
            .any(|widget| widget.has_css_class("receded"))
    };

    let first = holder("a");
    popover.set_entries(&[
        entry("a", BluetoothPlace::Connected),
        entry("b", BluetoothPlace::Paired),
    ]);
    assert_eq!(
        holder("a"),
        first,
        "a key that stays keeps its widget, and its card with it"
    );
    assert!(
        holder("a").head::<SplitRow>().is_none(),
        "a device in use opens its card from the whole row, so a click can never disconnect it"
    );
    assert!(
        holder("b")
            .head::<SplitRow>()
            .is_some_and(|split| split.detail().get_visible()),
        "a paired device connects on its body and opens its card from the chevron"
    );

    let acted = Rc::new(RefCell::new(Vec::new()));
    popover.connect_acted({
        let acted = Rc::clone(&acted);
        move |_, id, action| acted.borrow_mut().push(format!("{id}/{action}"))
    });
    let activated = Rc::new(RefCell::new(Vec::new()));
    popover.connect_activated({
        let activated = Rc::clone(&activated);
        move |_, id| activated.borrow_mut().push(id.to_owned())
    });

    let trust = |on: bool| BluetoothLine {
        action: "trust".to_owned(),
        title: "Connect automatically".to_owned(),
        toggle: Some(on),
        ..Default::default()
    };
    let forget = BluetoothLine {
        action: "forget".to_owned(),
        title: "Forget this device".to_owned(),
        activates: true,
        ..Default::default()
    };
    let in_use = |on: bool| BluetoothDetails {
        id: "a".to_owned(),
        facts: vec![Fact::new("Battery", "72%")],
        lines: vec![
            trust(on),
            BluetoothLine {
                action: "disconnect".to_owned(),
                title: "Disconnect".to_owned(),
                activates: true,
                ..Default::default()
            },
            forget.clone(),
        ],
    };
    let paired = BluetoothDetails {
        id: "b".to_owned(),
        facts: vec![Fact::new("Type", "Headset")],
        lines: vec![trust(true), forget.clone()],
    };
    popover.set_details(&[in_use(false), paired.clone()]);
    assert!(
        !holder("a").expanded(),
        "handing the details over opens nothing"
    );

    holder("a")
        .head::<LegacyRow>()
        .expect("a plain row")
        .emit_clicked();
    assert!(
        holder("a").expanded() && activated.borrow().is_empty(),
        "the whole row of a device in use opens its card and reports nothing"
    );
    let a_head = holder("a").head::<gtk4::Widget>().expect("a head");
    let b_head = holder("b").head::<gtk4::Widget>().expect("a head");
    assert!(
        !dimmed(&a_head) && dimmed(&b_head) && dimmed(popover.imp().hero.upcast_ref()),
        "everything the card is read against recedes while it is open"
    );

    let lines = rows_of("a");
    assert_eq!(lines.len(), 3);
    assert!(
        lines[0].clone().downcast::<SwitchRow>().is_ok(),
        "a line carrying a toggle is a SwitchRow, not a row with a switch dropped in it"
    );
    assert!(
        lines[1].activatable() && lines[2].activatable(),
        "a line that acts keeps the pointer"
    );
    let facts = card_of("a")
        .first_child()
        .and_downcast::<FactList>()
        .expect("the facts lead the card");
    let toggled = Rc::new(RefCell::new(Vec::new()));
    popover.connect_toggled({
        let toggled = Rc::clone(&toggled);
        move |_, id, action, on| toggled.borrow_mut().push(format!("{id}/{action}/{on}"))
    });
    lines[0].emit_clicked();
    lines[0].emit_clicked();
    assert_eq!(
        *toggled.borrow(),
        ["a/trust/true", "a/trust/false"],
        "a toggle line acts through its own row body, exactly once per press and both ways"
    );
    assert!(
        children_of::<LegacyRow>(&facts)
            .iter()
            .all(|row| !row.activatable()),
        "a line that only states something must not light up under a hover leading nowhere"
    );
    lines[1].emit_by_name::<()>("clicked", &[]);
    assert_eq!(*acted.borrow(), ["a/disconnect"]);

    let switch = lines[0]
        .trail()
        .and_downcast::<gtk4::Switch>()
        .expect("a toggle row carries a switch");
    assert!(!switch.is_active());
    popover.set_details(&[in_use(true), paired.clone()]);
    assert!(
        switch.is_active(),
        "a reused row must follow the backend, not the last click"
    );
    assert_eq!(
        *acted.borrow(),
        ["a/disconnect"],
        "redressing the switch is not the user toggling it"
    );

    holder("b")
        .head::<SplitRow>()
        .expect("a split row")
        .emit_by_name::<()>("details", &[]);
    assert!(
        !holder("a").expanded() && holder("b").expanded(),
        "opening one device closes the one that was open"
    );
    acted.borrow_mut().clear();
    rows_of("b")[1].emit_by_name::<()>("clicked", &[]);
    assert_eq!(
        *acted.borrow(),
        ["b/forget"],
        "a row whose action key repeats across devices must not act on the one no longer shown"
    );
    popover.collapse("b");
    assert!(
        !holder("b").expanded(),
        "an action that succeeded closes the card it was taken from"
    );
    holder("b")
        .head::<SplitRow>()
        .expect("a split row")
        .emit_by_name::<()>("activated", &[]);
    assert_eq!(
        *activated.borrow(),
        ["b"],
        "a paired device's body connects"
    );

    assert!(
        holder("b")
            .head::<SplitRow>()
            .is_some_and(|split| split.row().subtitle().is_none()),
        "a device row is one line"
    );

    let pressed = Rc::new(Cell::new(0u32));
    popover.connect_nearby_toggled({
        let pressed = Rc::clone(&pressed);
        move |_| pressed.set(pressed.get() + 1)
    });

    popover.set_nearby(0, true);
    assert!(
        !popover.imp().nearby.get_visible(),
        "nothing found is no header at all, never an open list over nothing"
    );
    popover.set_entries(&[
        entry("a", BluetoothPlace::Connected),
        entry("b", BluetoothPlace::Paired),
        entry("n", BluetoothPlace::Nearby),
    ]);
    popover.set_nearby(1, false);
    assert!(
        popover.imp().nearby.get_visible()
            && !popover.imp().nearby_list.reveals_child()
            && !popover.imp().search.has_css_class("open"),
        "closed, Nearby devices is one header line, the same as Other networks"
    );
    popover.set_nearby(1, true);
    assert!(
        popover.imp().nearby_list.reveals_child() && popover.imp().search.has_css_class("open"),
        "open, its chevron turns and the list shows"
    );
    let nearby = holder("n").head::<SplitRow>().expect("a split row");
    assert!(
        nearby.detail().get_visible(),
        "a nearby device's card names what it is, so it has a chevron"
    );
    nearby.emit_by_name::<()>("activated", &[]);
    assert_eq!(
        *activated.borrow(),
        ["b", "n"],
        "its body is the one action: pair it"
    );
    assert_eq!(
        pressed.get(),
        0,
        "redressing the header is not the user pressing it"
    );
    popover.imp().search.emit_clicked();
    assert_eq!(pressed.get(), 1, "the header is the toggle");

    popover.set_controls_sensitive(false);
    assert!(!popover.imp().search.get_sensitive());
    popover.set_controls_sensitive(true);

    let width = |popover: &BluetoothPopover| popover.measure(gtk4::Orientation::Horizontal, -1).1;
    {
        let probe = BluetoothPopover::new();
        probe.set_adapter(
            "Bluetooth",
            "No device connected",
            "bluetooth-symbolic",
            true,
            true,
        );
        probe.set_entries(&[entry("a", BluetoothPlace::Connected)]);
        let floor = width(&probe);

        probe.set_adapter(
            "Bluetooth",
            "Connected to Bose QuietComfort Ultra and 2 others",
            "bluetooth-active-symbolic",
            true,
            true,
        );
        assert_eq!(
            width(&probe),
            floor,
            "a hero subtitle that grows with the device list widened the whole card"
        );

        probe.set_details(&[BluetoothDetails {
            id: "a".to_owned(),
            facts: vec![Fact::new(
                "Codec",
                "Audio, Calls, Remote control, Network, File transfer",
            )],
            lines: Vec::new(),
        }]);
        let opened = probe
            .imp()
            .devices_held
            .borrow()
            .first()
            .map(|(_, holder)| holder.clone())
            .expect("the device");
        opened
            .head::<LegacyRow>()
            .expect("a plain row")
            .emit_clicked();
        assert!(opened.expanded());
        assert_eq!(
            width(&probe),
            floor,
            "a row's value has to stop asking for room at some point, or opening a device \
                 resizes the card under the pointer that opened it"
        );

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
        assert!(
            !opened.expanded(),
            "a prompt takes the card over, so a device card left open behind it would dim it"
        );
        assert_eq!(
            width(&probe),
            floor,
            "a prompt taking over the card must not resize the surface it took over"
        );
        assert_eq!(
            probe.imp().pages.visible_child_name().as_deref(),
            Some("prompt")
        );
        assert!(
            !probe.imp().hero.get_sensitive() && !probe.imp().footer.get_sensitive(),
            "a prompt owns the surface, so nothing behind it stays pressable"
        );
        assert!(probe.imp().prompt_accept.get_visible());
        assert!(probe.imp().prompt_code.get_visible());

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
        assert_eq!(
            width(&probe),
            floor,
            "a code the peer chose must not be able to widen the card"
        );

        probe.imp().prompt_accept.emit_by_name::<()>("clicked", &[]);
        probe.imp().prompt_cancel.emit_by_name::<()>("clicked", &[]);
        assert_eq!(*answers.borrow(), [true, false]);

        probe.set_prompt(Some(&BluetoothAsk {
            key: "pair:/org/bluez/hci0/dev_b".to_owned(),
            device: "UE BOOM 3".to_owned(),
            question: "This device wants to pair with this computer.".to_owned(),
            code: String::new(),
            progress: String::new(),
            accept: String::new(),
            cancel: "Deny".to_owned(),
            destructive: false,
        }));
        assert!(
            !probe.imp().prompt_accept.get_visible(),
            "a prompt with nothing to accept offers only the way out"
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
            Some("devices"),
            "an answered prompt hands the card back to the device list"
        );
        assert!(probe.imp().hero.get_sensitive() && probe.imp().footer.get_sensitive());
    }
}
