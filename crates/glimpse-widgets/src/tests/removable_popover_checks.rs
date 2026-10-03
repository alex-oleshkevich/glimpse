use super::*;

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

    let photos = |mounted: bool| RemovableVolume {
        id: "photos".to_owned(),
        title: "Photos".to_owned(),
        fraction: mounted.then_some(0.625),
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
                title: "Backup".to_owned(),
                busy: backup_busy,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let expandables = || children_of::<Expandable>(&*imp.devices_rows);
    let card_rows = |holder: &Expandable| {
        let Some(card) = holder.details::<gtk4::Box>() else {
            return Vec::new();
        };
        let mut rows = children_of::<LegacyRow>(&card);
        for facts in children_of::<FactList>(&card) {
            rows.extend(children_of::<LegacyRow>(&facts));
        }
        rows
    };
    let titled = |holder: &Expandable, title: &str| {
        card_rows(holder)
            .into_iter()
            .find(|row| row.title().as_deref() == Some(title))
    };

    let activated = Rc::new(RefCell::new(Vec::<String>::new()));
    popover.connect_activated({
        let activated = Rc::clone(&activated);
        move |_, id| activated.borrow_mut().push(id.to_owned())
    });
    let ejected = Rc::new(RefCell::new(None));
    popover.connect_eject({
        let ejected = Rc::clone(&ejected);
        move |_, id| {
            ejected.replace(Some(id.to_owned()));
        }
    });
    let unmounted = Rc::new(RefCell::new(None));
    popover.connect_unmount({
        let unmounted = Rc::clone(&unmounted);
        move |_, id| {
            unmounted.replace(Some(id.to_owned()));
        }
    });

    popover.set_devices(&[cruzer(photos(true), false)]);
    assert!(imp.devices.get_visible());
    let cells = expandables();
    assert_eq!(cells.len(), 3, "one drive header plus its two volumes");
    assert_eq!(cells[0].margin_start(), 0);
    assert_eq!(
        cells[1].margin_start(),
        24,
        "a volume nests under its drive"
    );

    let header = cells[0]
        .head::<LegacyRow>()
        .expect("the drive row opens a card");
    header.emit_by_name::<()>("clicked", &[]);
    assert!(cells[0].expanded(), "a drive row opens its card");
    titled(&cells[0], "Eject")
        .expect("the drive's card ejects")
        .emit_by_name::<()>("clicked", &[]);
    assert_eq!(ejected.borrow().as_deref(), Some("cruzer"));
    assert!(
        titled(&cells[0], "Size").is_some(),
        "the drive's facts sit in its card"
    );

    let mounted = cells[1]
        .head::<LegacyRow>()
        .expect("a mounted volume is a plain opener");
    mounted.emit_by_name::<()>("clicked", &[]);
    assert!(cells[1].expanded(), "a volume in use opens its card");
    assert!(
        activated.borrow().is_empty(),
        "opening the card is not opening the files"
    );
    let bar = cells[1]
        .details::<gtk4::Box>()
        .and_then(|card| card.first_child())
        .and_downcast::<gtk4::ProgressBar>()
        .expect("the capacity bar leads the card");
    assert!((bar.fraction() - 0.625).abs() < f64::EPSILON);
    titled(&cells[1], "Open")
        .expect("an Open row")
        .emit_by_name::<()>("clicked", &[]);
    assert_eq!(activated.borrow().as_slice(), ["cruzer/photos"]);
    titled(&cells[1], "Unmount")
        .expect("a volume of a shared drive unmounts rather than ejects")
        .emit_by_name::<()>("clicked", &[]);
    assert_eq!(unmounted.borrow().as_deref(), Some("photos"));
    assert!(titled(&cells[1], "Eject").is_none());

    let idle = cells[2]
        .head::<LegacyRow>()
        .expect("nothing to show makes a plain row");
    assert!(
        cells[2].details::<gtk4::Widget>().is_none(),
        "an unmounted volume with no facts has no card"
    );
    idle.emit_by_name::<()>("clicked", &[]);
    assert_eq!(
        activated.borrow().last().map(String::as_str),
        Some("cruzer/backup"),
        "clicking an unmounted volume mounts it"
    );

    let card = cells[1].details::<gtk4::Widget>();
    let mut grown = photos(true);
    grown.fraction = Some(0.75);
    popover.set_devices(&[cruzer(grown, true)]);
    let after = expandables();
    assert_eq!(after[1], cells[1], "an unrelated update keeps the same row");
    assert_eq!(
        after[1].details::<gtk4::Widget>(),
        card,
        "a capacity change updates the bar in place rather than rebuilding the card"
    );
    assert!((bar.fraction() - 0.75).abs() < f64::EPSILON);
    assert!(
        after[2].head::<LegacyRow>().expect("a row").busy(),
        "work in flight spins"
    );

    popover.set_devices(&[cruzer(photos(false), false)]);
    let after = expandables();
    let split = after[1]
        .head::<SplitRow>()
        .expect("an unmounted volume with a card mounts from its row");
    assert!(!after[1].expanded(), "a successful unmount closes the card");
    split.row().emit_by_name::<()>("clicked", &[]);
    assert_eq!(
        activated.borrow().last().map(String::as_str),
        Some("cruzer/photos")
    );
    split.detail().emit_by_name::<()>("clicked", &[]);
    assert!(after[1].expanded(), "the chevron opens the card");
    assert!(titled(&after[1], "Open").is_none());
    assert!(titled(&after[1], "Unmount").is_none());

    popover.set_devices(&[RemovableDrive {
        id: "stick".to_owned(),
        title: "KINGSTON".to_owned(),
        ejectable: true,
        volumes: vec![RemovableVolume {
            id: "vol".to_owned(),
            title: "KINGSTON".to_owned(),
            mounted: true,
            problem: "No space left".to_owned(),
            ..Default::default()
        }],
        ..Default::default()
    }]);
    let single = &expandables()[0];
    assert!(
        single
            .head::<LegacyRow>()
            .expect("a row")
            .has_css_class("row--warning")
    );
    titled(single, "Eject")
        .expect("a drive's only volume ejects the drive")
        .emit_by_name::<()>("clicked", &[]);
    assert_eq!(ejected.borrow().as_deref(), Some("stick"));
    assert_eq!(
        card_rows(single)
            .iter()
            .filter(|row| row.has_css_class("row--destructive"))
            .count(),
        1,
        "Eject is the card's one red action"
    );

    popover.set_devices(&[RemovableDrive {
        id: "dvd".to_owned(),
        title: "DVD-RW".to_owned(),
        ejectable: true,
        dimmed: true,
        ..Default::default()
    }]);
    let empty = &expandables()[0];
    assert!(empty.details::<gtk4::Widget>().is_none());
    assert!(!empty.head::<LegacyRow>().expect("a row").activatable());

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
    popover.set_devices(&[drive("d", "Backup")]);
    let floor = width(&popover);

    let hostile = "\u{1f642}".repeat(400);
    popover.set_devices(&[drive("d", &hostile)]);
    assert_eq!(
        width(&popover),
        floor,
        "a 400-character volume label must not widen the card"
    );

    let more = Rc::new(Cell::new(false));
    popover.connect_more({
        let more = Rc::clone(&more);
        move |_| more.set(true)
    });
    popover.set_overflow(Some("3 more"));
    imp.devices_more.emit_by_name::<()>("clicked", &[]);
    assert!(more.get());

    let footer_activated = Rc::new(Cell::new(false));
    popover.connect_footer_activated({
        let footer_activated = Rc::clone(&footer_activated);
        move |_| footer_activated.set(true)
    });
    popover.set_footer(Some("Open file manager"));
    assert!(imp.footer.get_visible());
    imp.footer.emit_by_name::<()>("clicked", &[]);
    assert!(footer_activated.get());
}
