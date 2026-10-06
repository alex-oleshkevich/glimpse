use super::*;

pub(super) fn slots(row: LegacyRow) {
    let check = child_named::<gtk4::Image>(&row, "row__check");
    let row_title = child_named::<gtk4::Label>(&row, "row__title");
    let row_subtitle = child_named::<gtk4::Label>(&row, "row__subtitle");

    assert!(
        !check.is_visible(),
        "a row that cannot be selected spends no width on the column"
    );
    row.set_selectable(true);
    assert!(
        check.is_visible() && check.icon_name().is_none(),
        "a selectable row reserves the column before anything is selected, so a later \
             selection does not shift the label"
    );
    row.set_selected(true);
    assert_eq!(check.icon_name().as_deref(), Some("object-select-symbolic"));
    assert!(row.has_css_class("row--on"));
    row.set_selected(false);
    assert!(check.icon_name().is_none() && !row.has_css_class("row--on"));

    assert!(!row_subtitle.is_visible());
    row.set_title(Some("Tenda_4A21F0"));
    row.set_subtitle(Some("WPA2 · 5 GHz"));
    assert!(row_subtitle.is_visible());
    row.set_subtitle(None::<&str>);
    assert!(!row_subtitle.is_visible());

    row.set_title(Some("ё".repeat(TEXT_MAX_CHARS * 2)));
    assert_eq!(
        row_title.text().chars().count(),
        TEXT_MAX_CHARS,
        "an SSID is another application's string: capped without slicing a character"
    );

    let row_icon = child_named::<gtk4::Image>(&row, "row__icon");
    let row_value = child_named::<gtk4::Label>(&row, "row__value");
    assert!(
        !row_icon.get_visible() && !row_value.get_visible(),
        "a row with neither reserves space for neither"
    );
    row.set_lead_icon(Some("network-wireless-symbolic"));
    row.set_value(Some("WPA3"));
    assert!(row_icon.get_visible() && row_value.get_visible());

    let icon_changes = Rc::new(Cell::new(0u32));
    row_icon.connect_icon_name_notify({
        let icon_changes = Rc::clone(&icon_changes);
        move |_| icon_changes.set(icon_changes.get() + 1)
    });
    row.set_lead_icon(Some("network-wireless-symbolic"));
    assert_eq!(
        icon_changes.get(),
        0,
        "an equal icon name is not reapplied, so a list re-rendering itself restyles nothing"
    );

    row.set_value(Some("ё".repeat(TEXT_MAX_CHARS * 2)));
    assert_eq!(
        row_value.text().chars().count(),
        TEXT_MAX_CHARS,
        "a value is another application's string too"
    );
    row.set_value(None::<&str>);
    assert!(
        !row_value.get_visible(),
        "a cleared value gives its width back rather than leaving a gap before the chevron"
    );
    row.set_lead_icon(None::<&str>);

    let signal = gtk4::Image::from_icon_name("network-wireless-symbolic");
    let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
    let reparented = Rc::new(Cell::new(0u32));
    signal.connect_parent_notify({
        let reparented = Rc::clone(&reparented);
        move |_| reparented.set(reparented.get() + 1)
    });
    row.set_lead(&signal);
    row.set_lead(&signal);
    assert_eq!(
        reparented.get(),
        1,
        "filling a slot with the widget already in it does not unparent and reparent it, \
             which is what a list re-rendering the same rows would otherwise do every update"
    );
    row.set_trail(&chevron);
    assert_eq!(
        signal.parent().and_then(|slot| slot.parent()),
        chevron.parent().and_then(|slot| slot.parent()),
        "both slots hang off the same row"
    );
    row.clear_trail();
    assert!(chevron.parent().is_none());
    assert!(
        signal.parent().is_some(),
        "clearing one slot leaves the other alone"
    );

    row.set_title(Some("W".repeat(40)));
    let wide = row.measure(gtk4::Orientation::Horizontal, -1).1;
    row.set_title(Some("W".repeat(120)));
    assert_eq!(
        row.measure(gtk4::Orientation::Horizontal, -1).1,
        wide,
        "past the cap a longer title asks for no more width, so an SSID cannot widen the \
             popover it sits in. `ellipsize` alone does not do this — it lowers the minimum \
             width and leaves the natural width at the full string."
    );
}

pub(super) fn activation(row: LegacyRow) {
    assert!(row.can_target() && row.activatable());
    row.set_activatable(false);
    assert!(
        !row.can_target() && !row.can_focus(),
        "a row that does nothing takes neither the pointer nor the focus, so it cannot \
             light up under a hover that leads nowhere"
    );
}

pub(super) fn kinds() {
    for make in [
        (|| LegacyRow::new().upcast::<gtk4::Widget>()) as fn() -> gtk4::Widget,
        || Notice::new().upcast(),
        || PlayerRow::new().upcast(),
        || ForecastDay::new().upcast(),
        || ForecastHour::new().upcast(),
        || Hero::new().upcast(),
    ] {
        drop(make());
    }
}

pub(super) fn beside() {
    let quiet = LegacyRow::new();
    quiet.set_title(Some("Quiet"));
    let end = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let knob = gtk4::Switch::new();
    end.append(&knob);
    let host = crate::beside(&quiet, &end);
    quiet.set_activatable(false);
    assert!(!quiet.can_target() && !quiet.can_focus());
    assert!(!end.is_ancestor(&quiet));
    assert!(knob.can_focus() && knob.can_target());
    let window = gtk4::Window::new();
    window.set_child(Some(&host));
    window.set_visible(true);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !host.is_mapped() && std::time::Instant::now() < deadline {
        if !gtk4::glib::MainContext::default().iteration(false) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    assert!(host.is_mapped(), "the row host is mapped");
    let bounds = knob.compute_bounds(&host).expect("the switch has bounds");
    let picked = host
        .pick(
            f64::from(bounds.x() + bounds.width() / 2.0),
            f64::from(bounds.y() + bounds.height() / 2.0),
            gtk4::PickFlags::DEFAULT,
        )
        .expect("an inert row leaves its end switch clickable");
    assert!(
        picked == knob.clone().upcast::<gtk4::Widget>() || picked.is_ancestor(&knob),
        "an inert row leaves its end switch clickable, picked {picked:?}"
    );
    window.close();
}
