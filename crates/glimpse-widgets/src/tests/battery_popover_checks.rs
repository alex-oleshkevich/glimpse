use super::*;

pub(super) fn check() {
    let battery = BatteryPopover::new();
    assert!(
        battery
            .layout_manager()
            .is_some_and(|layout| layout.is::<gtk4::BinLayout>()),
        "the battery popover root has a bin layout"
    );
    battery.set_heading(
        Some("battery-full-charged-symbolic"),
        Some("Fully charged"),
        Some(100),
        None,
    );
    assert_eq!(battery.imp().readout.value().as_deref(), Some("100"));
    assert!(battery.imp().readout.get_visible());
    battery.set_heading(
        Some("battery-level-10-symbolic"),
        Some("12 m left"),
        Some(9),
        Some(Severity::Error),
    );
    assert!(
        battery
            .imp()
            .hero
            .has_css_class("battery-popover__hero--error")
            && !battery
                .imp()
                .hero
                .has_css_class("battery-popover__hero--warning"),
        "a critical battery colors the hero as the chip does"
    );
    battery.set_profiles(&[], None);
    assert!(!battery.imp().profiles_section.get_visible());
    battery.set_profiles(
        &[Choice {
            label: "Balanced".to_owned(),
            detail: String::new(),
            icon_name: "power-profile-balanced-symbolic".to_owned(),
            warning: false,
        }],
        Some(0),
    );
    assert!(battery.imp().profiles_section.get_visible());
    battery.set_devices(&[]);
    assert!(!battery.imp().devices_section.get_visible());
    battery.set_devices(&[BatteryDevice {
        name: "MX Master 3S".to_owned(),
        subtitle: String::new(),
        icon_name: "input-mouse-symbolic".to_owned(),
        value: "12%".to_owned(),
        warning: true,
    }]);
    assert!(battery.imp().devices_section.get_visible());
    assert!(
        children_of::<LegacyRow>(&*battery.imp().devices_box)[0].has_css_class("row--warning"),
        "a device running low reads amber"
    );
    battery.set_health(None, false, &[]);
    assert!(!battery.imp().details.get_visible());
    assert!(!battery.imp().details.expanded());
    battery.set_charge_limit(None);
    assert!(!battery.imp().charge_limit.get_visible());
    battery.set_health(Some("74%"), true, &[Fact::new("Cycles", "812")]);
    assert!(battery.imp().details.get_visible());
    assert_eq!(battery.imp().details_row.value().as_deref(), Some("74%"));
    assert!(battery.imp().details_row.has_css_class("row--warning"));
    assert!(
        battery.imp().details_row.subtitle().is_none(),
        "the health row carries its value and nothing under it"
    );
    battery.set_charge_limit(Some(&BatteryChargeLimit {
        enabled: false,
        title: "Limit charge to 80%".to_owned(),
        subtitle: "Slows battery wear".to_owned(),
    }));
    assert!(battery.imp().charge_limit.get_visible());
    assert!(!battery.imp().charge_limit.active());
    assert!(
        !battery
            .imp()
            .charge_limit
            .is_ancestor(&*battery.imp().details),
        "the charge limit is a control in the column, never inside the health card"
    );
    battery.imp().details_row.emit_clicked();
    assert!(battery.imp().details.expanded());
    battery.set_health(None, false, &[]);
    assert!(
        !battery.imp().details.expanded(),
        "a card whose details go away closes"
    );
    let battery_profiles = Rc::new(Cell::new(None));
    battery.connect_profile_activated({
        let battery_profiles = Rc::clone(&battery_profiles);
        move |_, index| battery_profiles.set(Some(index))
    });
    battery
        .imp()
        .profiles
        .emit_by_name::<()>("activated", &[&0u32]);
    assert_eq!(battery_profiles.get(), Some(0));
    let battery_limit = Rc::new(Cell::new(None));
    battery.connect_charge_limit_toggled({
        let battery_limit = Rc::clone(&battery_limit);
        move |_, on| battery_limit.set(Some(on))
    });
    battery
        .imp()
        .charge_limit
        .emit_by_name::<()>("toggled", &[&true]);
    assert_eq!(battery_limit.get(), Some(true));
}
