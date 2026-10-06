use super::*;

use crate::removable_popover::RemovableDevice;

/// Separate from `widgets()` for the same reason as `places_popover_widgets`: one failure
/// earlier in that function must not hide every assertion written after it.
#[test]
#[ignore = "needs a display"]
fn removable_popover_widgets() {
    if gtk4::init().is_err() {
        return;
    }
    register_resources().expect("resources");
    let _styles = Styles::install(adw::ColorScheme::Default);

    let popover = RemovablePopover::new();
    let imp = popover.imp();

    assert!(
        !imp.devices.get_visible(),
        "an untouched popover lists no device"
    );
    assert_eq!(imp.shell.title().as_deref(), Some("Removable"));
    assert!(
        imp.shell.icon_name().is_none(),
        "the header carries no icon"
    );

    let photos = |mounted: bool| RemovableVolume {
        id: "photos".to_owned(),
        title: "Cruzer \u{b7} Photos".to_owned(),
        icon: "drive-multidisk-symbolic".to_owned(),
        fraction: mounted.then_some(0.625),
        space: "24 GB free of 64 GB".to_owned(),
        mounted,
        facts: vec![Fact::new("Filesystem", "exfat")],
        ..Default::default()
    };
    let cruzer = |photos: RemovableVolume, backup_busy: bool| RemovableDrive {
        id: "cruzer".to_owned(),
        title: "SanDisk Cruzer".to_owned(),
        ejectable: true,
        facts: vec![Fact::new("Size", "128 GB")],
        volumes: vec![
            photos,
            RemovableVolume {
                id: "backup".to_owned(),
                title: "Cruzer \u{b7} Backup".to_owned(),
                icon: "drive-multidisk-symbolic".to_owned(),
                busy: backup_busy,
                facts: vec![Fact::new("Status", "Not mounted")],
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let show =
        |popover: &RemovablePopover, drives: &[RemovableDrive]| popover.set_devices(drives, &[]);
    let devices = || {
        imp.devices_held
            .borrow()
            .iter()
            .map(|(_, device)| device.clone())
            .collect::<Vec<RemovableDevice>>()
    };
    let drawer = |device: &RemovableDevice| {
        let mut shown = Vec::new();
        let mut child = device.imp().row.imp().details.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if widget.get_visible() {
                shown.push(widget.type_().name().to_owned());
            }
        }
        shown
    };

    let activated = Rc::new(RefCell::new(Vec::<String>::new()));
    popover.connect_activated({
        let activated = Rc::clone(&activated);
        move |_, id| activated.borrow_mut().push(id.to_owned())
    });
    let ejected = Rc::new(RefCell::new(Vec::<String>::new()));
    popover.connect_eject({
        let ejected = Rc::clone(&ejected);
        move |_, id| ejected.borrow_mut().push(id.to_owned())
    });
    let unmounted = Rc::new(RefCell::new(Vec::<String>::new()));
    popover.connect_unmount({
        let unmounted = Rc::clone(&unmounted);
        move |_, id| unmounted.borrow_mut().push(id.to_owned())
    });

    show(&popover, &[cruzer(photos(true), false)]);
    assert!(imp.devices.get_visible());
    let cells = devices();
    assert_eq!(
        cells.len(),
        2,
        "a flat list: one row per volume, no drive header"
    );
    let first = cells[0].imp();
    assert_eq!(first.row.title().as_deref(), Some("Cruzer \u{b7} Photos"));
    assert_eq!(
        first.row.icon_name().as_deref(),
        Some("drive-multidisk-symbolic")
    );
    assert!(!first.row.inactive(), "a mounted volume is not dimmed");
    assert_eq!(
        drawer(&cells[0]),
        ["MeterRow", "FactList", "ActionRow", "ActionRow"],
        "capacity, facts, the primary action, then the release last"
    );
    assert!((first.space.level() - 0.625).abs() < f64::EPSILON);
    assert_eq!(
        first.space.upcast_ref::<Row>().value().as_deref(),
        Some("24 GB free of 64 GB")
    );
    assert_eq!(first.space.upcast_ref::<Row>().tone(), RowTone::Normal);
    assert_eq!(
        first.primary.upcast_ref::<Row>().title().as_deref(),
        Some("Open")
    );
    assert_eq!(
        first.release.upcast_ref::<Row>().title().as_deref(),
        Some("Eject")
    );
    assert_eq!(
        first.release.upcast_ref::<Row>().tone(),
        RowTone::Destructive,
        "Eject is the drawer's one red action"
    );
    assert!(
        first.release.next_sibling().is_none(),
        "the destructive action is the last child of the drawer"
    );

    first.row.set_open(true);
    first.primary.emit_by_name::<()>("activated", &[]);
    assert_eq!(activated.borrow().as_slice(), ["cruzer/photos"]);
    first.release.emit_by_name::<()>("activated", &[]);
    assert_eq!(ejected.borrow().as_slice(), ["cruzer"]);
    assert!(unmounted.borrow().is_empty());

    let idle = cells[1].imp();
    assert!(idle.row.inactive(), "an unmounted volume is dimmed");
    assert!(idle.row.get_sensitive(), "a dimmed head stays openable");
    assert!(
        idle.row.imp().head.is_sensitive(),
        "the head itself takes the press that opens the drawer"
    );
    assert_eq!(
        drawer(&cells[1]),
        ["FactList", "ActionRow", "ActionRow"],
        "no capacity meter without a mount"
    );
    assert_eq!(
        idle.primary.upcast_ref::<Row>().title().as_deref(),
        Some("Mount")
    );
    assert_eq!(
        idle.release.upcast_ref::<Row>().title().as_deref(),
        Some("Eject")
    );
    idle.primary.emit_by_name::<()>("activated", &[]);
    assert_eq!(
        activated.borrow().last().map(String::as_str),
        Some("cruzer/backup")
    );

    let held = cells[0].clone();
    let mut grown = photos(true);
    grown.fraction = Some(0.75);
    show(&popover, &[cruzer(grown, true)]);
    let after = devices();
    assert_eq!(after[0], held, "an unrelated update keeps the same row");
    assert!((after[0].imp().space.level() - 0.75).abs() < f64::EPSILON);
    assert!(after[1].imp().row.busy(), "work in flight spins");
    assert!(
        !after[1].imp().primary.get_visible(),
        "a busy volume offers no Open or Mount"
    );
    assert!(
        !after[1].imp().release.get_visible(),
        "a busy volume offers no release"
    );
    assert!(
        after[1].imp().row.subtitle().is_none(),
        "busy is a spinner, never wording"
    );

    show(
        &popover,
        &[RemovableDrive {
            id: "stick".to_owned(),
            title: "KINGSTON".to_owned(),
            ejectable: false,
            volumes: vec![RemovableVolume {
                id: "vol".to_owned(),
                title: "KINGSTON".to_owned(),
                icon: "media-flash-symbolic".to_owned(),
                mounted: true,
                fraction: Some(1.0),
                space: "0 B free of 16 GB".to_owned(),
                problem: "No space left".to_owned(),
                facts: vec![Fact::new("Filesystem", "fat32")],
                ..Default::default()
            }],
            ..Default::default()
        }],
    );
    let single = &devices()[0];
    let warned = single.imp();
    assert_eq!(warned.row.tone(), RowTone::Warning);
    assert_eq!(warned.row.subtitle().as_deref(), Some("No space left"));
    assert_eq!(
        warned.space.upcast_ref::<Row>().tone(),
        RowTone::Warning,
        "a problem tints the capacity meter"
    );
    assert_eq!(
        warned.release.upcast_ref::<Row>().title().as_deref(),
        Some("Unmount"),
        "a mounted volume of a drive that cannot eject unmounts"
    );
    warned.release.emit_by_name::<()>("activated", &[]);
    assert_eq!(unmounted.borrow().as_slice(), ["vol"]);

    let value_color = |row: &Row| row.imp().value.color();
    let fact = children_of::<Row>(&*warned.facts)
        .into_iter()
        .next()
        .expect("a fact row");
    let head_value = warned.row.imp().head.upcast_ref::<Row>().clone();
    head_value.set_value(Some("x"));
    let plain = RemovablePopover::new();
    show(
        &plain,
        &[RemovableDrive {
            id: "ok".to_owned(),
            title: "OK".to_owned(),
            volumes: vec![RemovableVolume {
                id: "v".to_owned(),
                title: "OK".to_owned(),
                mounted: true,
                facts: vec![Fact::new("Filesystem", "fat32")],
                ..Default::default()
            }],
            ..Default::default()
        }],
    );
    let plain_fact = children_of::<Row>(&*plain.imp().devices_held.borrow()[0].1.imp().facts)
        .into_iter()
        .next()
        .expect("a fact row");
    assert_eq!(
        value_color(&fact),
        value_color(&plain_fact),
        "a fact inside a warning drawer is not tinted by the head's tone"
    );
    assert_ne!(
        value_color(&head_value),
        value_color(&plain_fact),
        "the head's own value is tinted"
    );

    show(
        &popover,
        &[RemovableDrive {
            id: "dvd".to_owned(),
            title: "DVD-RW".to_owned(),
            icon: "drive-optical-symbolic".to_owned(),
            ejectable: true,
            dimmed: true,
            facts: vec![Fact::new("Status", "No media")],
            ..Default::default()
        }],
    );
    let empty = &devices()[0];
    assert!(empty.imp().row.inactive());
    assert_eq!(
        drawer(empty),
        ["FactList"],
        "a drive without media shows its status and offers no release"
    );

    show(
        &popover,
        &[RemovableDrive {
            id: "bare".to_owned(),
            title: "Bare".to_owned(),
            ..Default::default()
        }],
    );
    assert!(
        !devices()[0].imp().row.get_sensitive(),
        "a row with an empty drawer does not open"
    );

    let width = |popover: &RemovablePopover| popover.measure(gtk4::Orientation::Horizontal, -1).1;
    let drive = |id: &str, label: &str| RemovableDrive {
        id: id.to_owned(),
        title: label.to_owned(),
        volumes: vec![RemovableVolume {
            id: "v".to_owned(),
            title: label.to_owned(),
            ..Default::default()
        }],
        ..Default::default()
    };
    show(&popover, &[drive("d", "Backup")]);
    let floor = width(&popover);

    let hostile = "\u{1f642}".repeat(400);
    show(&popover, &[drive("d", &hostile)]);
    assert_eq!(
        width(&popover),
        floor,
        "a 400-character volume label must not widen the card"
    );

    let more = Rc::new(Cell::new(0));
    popover.connect_more({
        let more = Rc::clone(&more);
        move |_, open| more.set(more.get() + 1 + usize::from(open) * 10)
    });
    assert!(!imp.devices_more.get_visible(), "no overflow, no header");
    let spare = |id: &str| RemovableDrive {
        id: id.to_owned(),
        title: id.to_owned(),
        volumes: vec![RemovableVolume {
            id: "v".to_owned(),
            title: id.to_owned(),
            ..Default::default()
        }],
        ..Default::default()
    };
    popover.set_devices(&[spare("a")], &[spare("b"), spare("c")]);
    popover.set_overflow(Some("2 more"), false);
    assert!(imp.devices_more.get_visible());
    assert_eq!(imp.devices_more.title().as_deref(), Some("2 more"));
    assert_eq!(
        children_of::<RemovableDevice>(&*imp.more_rows).len(),
        2,
        "the overflow volumes are the same row widget, inside the header's drawer"
    );
    assert!(!imp.devices_more.open());
    popover.set_overflow(Some("2 more"), true);
    assert!(imp.devices_more.open());
    assert_eq!(more.get(), 0, "setting the state from outside is quiet");
    imp.devices_more.set_open(false);
    assert_eq!(
        more.get(),
        1,
        "the user toggling the header reports it, closed"
    );
    imp.devices_more.set_open(true);
    assert_eq!(more.get(), 12, "and reports it open");

    popover.set_devices(&[spare("a"), spare("b")], &[spare("c")]);
    assert_eq!(children_of::<RemovableDevice>(&*imp.devices_rows).len(), 2);
    assert_eq!(children_of::<RemovableDevice>(&*imp.more_rows).len(), 1);
    let moved = Rc::new(RefCell::new(Vec::<String>::new()));
    popover.connect_activated({
        let moved = Rc::clone(&moved);
        move |_, id| moved.borrow_mut().push(id.to_owned())
    });
    let promoted = children_of::<RemovableDevice>(&*imp.devices_rows)[1].clone();
    promoted.imp().primary.emit_by_name::<()>("activated", &[]);
    assert_eq!(
        moved.borrow().as_slice(),
        ["b/v"],
        "a promoted row reports its own key"
    );
    popover.set_devices(&[spare("a")], &[spare("c"), spare("b")]);
    assert_eq!(children_of::<RemovableDevice>(&*imp.devices_rows).len(), 1);
    assert_eq!(
        children_of::<RemovableDevice>(&*imp.more_rows).len(),
        2,
        "a demoted row leaves the list and is not duplicated"
    );
    let demoted = children_of::<RemovableDevice>(&*imp.more_rows)[1].clone();
    demoted.imp().primary.emit_by_name::<()>("activated", &[]);
    assert_eq!(moved.borrow().last().map(String::as_str), Some("b/v"));
    popover.set_overflow(None, true);
    assert!(!imp.devices_more.get_visible());

    let footer_activated = Rc::new(Cell::new(false));
    popover.connect_footer_activated({
        let footer_activated = Rc::clone(&footer_activated);
        move |_| footer_activated.set(true)
    });
    popover.set_footer(Some("Open file manager"));
    assert!(imp.footer.get_visible());
    imp.footer.emit_by_name::<()>("activated", &[]);
    assert!(footer_activated.get());
}
