use super::*;

#[test]
#[ignore = "needs a display"]
fn kdeconnect_popover_widgets() {
    if gtk4::init().is_err() {
        return;
    }
    register_resources().expect("resources");
    let _styles = Styles::install(adw::ColorScheme::Default);

    let popover = KdeconnectPopover::new();
    let imp = popover.imp();
    assert!(
        !imp.devices.get_visible(),
        "an untouched popover lists no device"
    );
    assert!(!imp.nearby.get_visible(), "and has no nearby header");

    let action = |key: &str| KdeconnectAction {
        key: key.to_owned(),
        label: key.to_owned(),
        destructive: key == "unpair",
    };
    let phone = KdeconnectDevice {
        id: "b98d".to_owned(),
        title: "Pixel 10 Pro".to_owned(),
        value: "76%".to_owned(),
        actions: vec![action("ring"), action("unpair")],
    };
    let away = KdeconnectDevice {
        id: "3d9c".to_owned(),
        title: "Pixel 8".to_owned(),
        value: "Not connected".to_owned(),
        actions: vec![action("unpair")],
    };
    popover.set_devices(&[phone.clone(), away.clone()]);
    assert!(imp.devices.get_visible());
    let holders = children_of::<ExpanderRow>(&*imp.devices_rows);
    assert_eq!(holders.len(), 2);
    assert_eq!(holders[0].title().as_deref(), Some("Pixel 10 Pro"));
    assert_eq!(holders[0].subtitle(), None, "one line");
    assert_eq!(holders[0].value().as_deref(), Some("76%"));
    assert_eq!(holders[1].value().as_deref(), Some("Not connected"));

    let rows = children_of::<ActionRow>(&holders[0].details());
    assert_eq!(rows.len(), 2);

    let fired = Rc::new(RefCell::new(Vec::<(String, String)>::new()));
    popover.connect_action({
        let fired = Rc::clone(&fired);
        move |_, id, key| fired.borrow_mut().push((id.to_owned(), key.to_owned()))
    });
    rows[0].emit_by_name::<()>("activated", &[]);
    assert_eq!(
        fired.borrow().as_slice(),
        [("b98d".to_owned(), "ring".to_owned())]
    );

    holders[0].set_open(true);
    let mut charging = phone.clone();
    charging.value = "77%, charging".to_owned();
    popover.set_devices(&[charging, away.clone()]);
    let again = children_of::<ExpanderRow>(&*imp.devices_rows);
    assert_eq!(
        again[0], holders[0],
        "a device keeps its row across an update"
    );
    assert!(
        again[0].open(),
        "an open card stays open when only the reading moves"
    );
    assert_eq!(
        children_of::<ActionRow>(&again[0].details()),
        rows,
        "unchanged actions keep their rows, so the pointer's row is not swapped out"
    );

    popover.collapse("b98d");
    assert!(!again[0].open(), "collapse closes the card it names");

    popover.set_devices(&[away.clone(), phone.clone()]);
    let reordered = children_of::<ExpanderRow>(&*imp.devices_rows);
    assert_eq!(
        reordered[1], holders[0],
        "rows are keyed by id, not position"
    );

    let nearby = |id: &str, busy: bool| KdeconnectNearby {
        id: id.to_owned(),
        title: id.to_owned(),
        subtitle: String::new(),
        busy,
    };
    popover.set_nearby(
        &[nearby("desk", false), nearby("tab", true)],
        Some("3 more"),
    );
    assert!(imp.nearby.get_visible());
    assert!(!popover.nearby_open(), "nearby starts closed");
    popover.set_nearby_open(true);
    assert!(popover.nearby_open());
    assert!(imp.nearby_more.get_visible());
    let splits = children_of::<ActionRow>(&*imp.nearby_rows);
    assert!(
        splits[1].upcast_ref::<Row>().busy(),
        "a requested pairing spins"
    );

    let paired = Rc::new(RefCell::new(Vec::<String>::new()));
    popover.connect_pair({
        let paired = Rc::clone(&paired);
        move |_, id| paired.borrow_mut().push(id.to_owned())
    });
    splits[0].emit_by_name::<()>("activated", &[]);
    assert!(
        splits[0].upcast_ref::<Row>().busy(),
        "the row spins the moment it is pressed"
    );
    splits[0].emit_by_name::<()>("activated", &[]);
    assert_eq!(
        paired.borrow().as_slice(),
        ["desk".to_owned()],
        "a second press while it spins asks nothing"
    );
    popover.set_nearby(
        &[nearby("desk", false), nearby("tab", true)],
        Some("3 more"),
    );
    assert!(
        !splits[0].upcast_ref::<Row>().busy(),
        "an unchanged state still clears the guess, so a refused request never spins forever"
    );

    popover.set_nearby_open(false);
    assert!(!popover.nearby_open());

    popover.set_nearby(&[], None);
    assert!(
        !imp.nearby.get_visible(),
        "no header until something is found"
    );
}
