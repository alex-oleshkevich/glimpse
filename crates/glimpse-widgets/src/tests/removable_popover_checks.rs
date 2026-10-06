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

    let kingston = |main_mounted: bool, extra_mounted: bool, busy: bool| RemovableDrive {
        id: "kingston".to_owned(),
        title: "Kingston DataTraveler".to_owned(),
        icon: "drive-multidisk-symbolic".to_owned(),
        busy,
        mounted: main_mounted,
        space: main_mounted.then(|| RemovableSpace {
            title: "Ventoy".to_owned(),
            fraction: 0.29,
            text: "41.2 GB free".to_owned(),
        }),
        volumes: vec![RemovableVolume {
            id: "efi".to_owned(),
            title: "VTOYEFI".to_owned(),
            size: "32 MB".to_owned(),
            mounted: extra_mounted,
        }],
        open: Some("ventoy".to_owned()),
        removable: true,
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
    let extras = |device: &RemovableDevice| children_of::<ActionRow>(&*device.imp().extras);

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

    show(&popover, &[kingston(true, true, false)]);
    assert!(imp.devices.get_visible());
    let cells = devices();
    assert_eq!(cells.len(), 1, "one row per device, never one per volume");
    let first = cells[0].imp();
    assert_eq!(first.row.title().as_deref(), Some("Kingston DataTraveler"));
    assert_eq!(
        first.row.icon_name().as_deref(),
        Some("drive-multidisk-symbolic")
    );
    assert!(!first.row.inactive(), "nothing is dimmed");
    assert!(
        first.mounted.get_visible(),
        "a mounted main volume wears the check"
    );
    assert_eq!(
        drawer(&cells[0]),
        ["MeterRow", "GtkBox", "ActionRow", "ActionRow"],
        "space, the extra volumes, Open, then Eject last"
    );
    assert_eq!(
        first.space.upcast_ref::<Row>().title().as_deref(),
        Some("Ventoy")
    );
    assert!((first.space.level() - 0.29).abs() < f64::EPSILON);
    assert_eq!(
        first.space.upcast_ref::<Row>().value().as_deref(),
        Some("41.2 GB free")
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

    let rows = extras(&cells[0]);
    assert_eq!(rows.len(), 1);
    let efi: &Row = rows[0].upcast_ref();
    assert_eq!(efi.title().as_deref(), Some("VTOYEFI"));
    assert_eq!(efi.value().as_deref(), Some("32 MB"));
    let efi_value = child_named::<gtk4::Label>(efi, "row__value");
    assert!(
        efi_value.get_visible(),
        "the size stays visible beside the check"
    );
    assert!(
        efi.imp().end().get_visible() && efi.imp().end().first_child().is_some(),
        "a mounted extra volume wears the check"
    );

    first.row.set_open(true);
    first.primary.emit_by_name::<()>("activated", &[]);
    assert_eq!(activated.borrow().as_slice(), ["ventoy"]);
    rows[0].emit_by_name::<()>("activated", &[]);
    assert_eq!(activated.borrow().last().map(String::as_str), Some("efi"));
    first.release.emit_by_name::<()>("activated", &[]);
    assert_eq!(ejected.borrow().as_slice(), ["kingston"]);

    show(&popover, &[kingston(false, false, false)]);
    let idle = &devices()[0];
    assert_eq!(idle, &cells[0], "an update keeps the same row");
    assert!(
        !idle.imp().mounted.get_visible(),
        "unmounted wears no check"
    );
    assert!(
        idle.imp().row.get_sensitive(),
        "an unmounted head stays openable"
    );
    assert_eq!(
        drawer(idle),
        ["GtkBox", "ActionRow", "ActionRow"],
        "no meter without a mounted main volume"
    );
    let rows = extras(idle);
    assert!(
        rows[0]
            .upcast_ref::<Row>()
            .imp()
            .end()
            .first_child()
            .is_none(),
        "an unmounted extra volume wears no check"
    );

    show(&popover, &[kingston(true, true, true)]);
    let busy = &devices()[0];
    assert!(busy.imp().row.busy(), "work in flight spins the head");
    for row in [&busy.imp().primary, &busy.imp().release] {
        assert!(
            row.get_visible() && !row.get_sensitive(),
            "Open and Eject stay in place and take no second press"
        );
    }
    let rows = extras(busy);
    assert!(
        rows[0].get_visible() && !rows[0].get_sensitive(),
        "an extra volume stays in place and takes no press while busy"
    );
    assert!(
        busy.imp().row.subtitle().is_none(),
        "busy is a spinner, never wording"
    );

    let mut refused = kingston(true, false, false);
    refused.problem = "In use by an app".to_owned();
    show(&popover, &[refused.clone()]);
    let warned = devices()[0].clone();
    assert_eq!(warned.imp().row.tone(), RowTone::Warning);
    assert_eq!(
        warned.imp().row.subtitle().as_deref(),
        Some("In use by an app")
    );
    assert_eq!(
        warned.imp().space.upcast_ref::<Row>().tone(),
        RowTone::Warning,
        "a problem tints the space meter"
    );
    let mut full = kingston(true, false, false);
    full.space.as_mut().expect("space").fraction = 1.0;
    show(&popover, &[full]);
    assert_eq!(
        devices()[0].imp().space.upcast_ref::<Row>().tone(),
        RowTone::Warning,
        "a full volume tints the space meter"
    );

    let value_color = |row: &Row| row.imp().value.color();
    let lone_extra = |popover: &RemovablePopover| {
        let device = popover.imp().devices_held.borrow()[0].1.clone();
        extras(&device)[0].upcast_ref::<Row>().clone()
    };
    show(&popover, &[refused]);
    let tinted = lone_extra(&popover);
    let plain = RemovablePopover::new();
    show(&plain, &[kingston(true, false, false)]);
    let calm = lone_extra(&plain);
    assert_eq!(
        value_color(&tinted),
        value_color(&calm),
        "a value inside a warning drawer is not tinted by the head's tone"
    );
    let head_value = warned.imp().row.imp().head.upcast_ref::<Row>().clone();
    head_value.set_value(Some("x"));
    assert_ne!(
        value_color(&head_value),
        value_color(&calm),
        "the head's own value is tinted"
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

    show(&popover, &[]);
    assert!(
        !imp.devices.get_visible(),
        "no device, no section: an empty reader is never listed"
    );

    let width = |popover: &RemovablePopover| popover.measure(gtk4::Orientation::Horizontal, -1).1;
    let drive = |id: &str, label: &str| RemovableDrive {
        id: id.to_owned(),
        title: label.to_owned(),
        open: Some("v".to_owned()),
        ..Default::default()
    };
    show(&popover, &[drive("d", "Backup")]);
    let floor = width(&popover);

    let hostile = "\u{1f642}".repeat(400);
    show(&popover, &[drive("d", &hostile)]);
    assert_eq!(
        width(&popover),
        floor,
        "a 400-character device name must not widen the card"
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
        open: Some("v".to_owned()),
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
        ["v"],
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
    assert_eq!(moved.borrow().last().map(String::as_str), Some("v"));
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
