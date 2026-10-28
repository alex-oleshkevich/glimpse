use super::*;
use glimpse_dbus::upower::ChargeState;
use glimpse_services::Charge;

fn icon_name(spec: &IndicatorSpec) -> Option<String> {
    let icon = spec.icon.as_ref()?.downcast_ref::<gio::ThemedIcon>()?;
    icon.names().first().map(ToString::to_string)
}

fn charge(percentage: u8, warning: WarningLevel) -> Charge {
    Charge {
        percentage,
        state: ChargeState::Discharging,
        icon_name: String::new(),
        time_to_empty: None,
        time_to_full: None,
        energy_rate_mw: None,
        warning,
    }
}

fn on_battery(charge: Charge) -> BatteryState {
    BatteryState {
        display: Some(charge),
        ..Default::default()
    }
}

#[test]
fn the_battery_shows_its_charge_icon_and_percentage() {
    let charge = charge(84, WarningLevel::Discharging);
    let spec = battery(&on_battery(charge.clone())).expect("a battery slot");
    assert_eq!(icon_name(&spec), Some(charge.icon_name()));
    assert_eq!(spec.label.as_deref(), Some("84%"));
    assert_eq!(spec.severity, None);
    assert!(!spec.attention);
}

#[test]
fn a_battery_icon_named_by_upower_wins() {
    let mut charge = charge(40, WarningLevel::None);
    charge.icon_name = "battery-caution-symbolic".to_owned();
    let spec = battery(&on_battery(charge)).expect("a battery slot");
    assert_eq!(
        icon_name(&spec).as_deref(),
        Some("battery-caution-symbolic")
    );
}

#[test]
fn no_display_device_hides_the_battery() {
    assert!(battery(&BatteryState::default()).is_none());
}

#[test]
fn a_battery_warning_maps_to_severity_and_never_to_attention() {
    let cases = [
        (WarningLevel::Unknown, None),
        (WarningLevel::None, None),
        (WarningLevel::Discharging, None),
        (WarningLevel::Low, Some(Severity::Warning)),
        (WarningLevel::Critical, Some(Severity::Error)),
        (WarningLevel::Action, Some(Severity::Error)),
    ];
    for (warning, expected) in cases {
        let spec = battery(&on_battery(charge(5, warning))).expect("a battery slot");
        assert_eq!(spec.severity, expected, "{warning:?}");
        assert!(
            !spec.attention,
            "{warning:?} never pulses on the lock screen"
        );
    }
}

#[test]
fn same_compares_icon_label_and_severity() {
    let low = battery(&on_battery(charge(9, WarningLevel::Low)));
    assert!(same(low.as_ref(), low.clone().as_ref()));
    assert!(
        same(
            low.as_ref(),
            battery(&on_battery(charge(9, WarningLevel::Low))).as_ref()
        ),
        "a freshly built icon of the same name is the same"
    );
    assert!(same(None, None));
    assert!(!same(low.as_ref(), None));
    assert!(!same(
        low.as_ref(),
        battery(&on_battery(charge(8, WarningLevel::Low))).as_ref()
    ));
    assert!(!same(
        low.as_ref(),
        battery(&on_battery(charge(9, WarningLevel::Critical))).as_ref()
    ));
    let mut other_icon = charge(9, WarningLevel::Low);
    other_icon.icon_name = "battery-caution-symbolic".to_owned();
    assert!(!same(
        low.as_ref(),
        battery(&on_battery(other_icon)).as_ref()
    ));

    let mut slot = low.clone();
    assert!(
        !replace(&mut slot, low.clone()),
        "an equal spec is no change"
    );
    assert!(replace(&mut slot, None));
    assert!(slot.is_none());
}

fn full() -> Status {
    Status {
        battery: Some(IndicatorSpec::default()),
        layout: Some(IndicatorSpec::default()),
    }
}

#[test]
fn disabled_status_hides_the_battery_but_keeps_the_layout() {
    let shown = shown(&full(), false);
    assert!(shown.battery.is_none());
    assert!(shown.layout.is_some());
}

#[test]
fn enabled_status_passes_the_battery_through() {
    assert!(shown(&full(), true).battery.is_some());
}

fn layouts(codes: &[&str], current: Option<u8>) -> KeyboardLayouts {
    KeyboardLayouts {
        layouts: codes
            .iter()
            .map(|code| glimpse_services::KeyboardLayout {
                code: (*code).to_owned(),
                name: format!("{code} layout"),
            })
            .collect(),
        current,
    }
}

#[test]
fn fewer_than_two_layouts_hide_the_slot() {
    assert!(layout(&layouts(&[], None)).is_none());
    assert!(layout(&layouts(&["us"], Some(0))).is_none());
}

#[test]
fn two_layouts_show_the_current_code_and_name() {
    let spec = layout(&layouts(&["us", "cz"], Some(1))).expect("a layout slot");
    assert_eq!(spec.label.as_deref(), Some("cz"));
    assert!(
        spec.tooltip
            .is_some_and(|tooltip| tooltip.contains("cz layout"))
    );
    assert!(spec.icon.is_none());
}

#[test]
fn an_unknown_current_layout_falls_back_to_the_first() {
    let unset = layout(&layouts(&["us", "cz"], None)).expect("a layout slot");
    assert_eq!(unset.label.as_deref(), Some("us"));
    let out_of_range = layout(&layouts(&["us", "cz"], Some(9))).expect("a layout slot");
    assert_eq!(out_of_range.label.as_deref(), Some("us"));
}

#[test]
fn a_layout_code_is_cleaned_and_capped() {
    let spec = layout(&layouts(&["us", "a\u{202e}véry-long-layout-code"], Some(1)))
        .expect("a layout slot");
    let label = spec.label.expect("a label");
    assert_eq!(label.chars().count(), LAYOUT_CAP + 1, "{label}");
    assert!(!label.contains('\u{202e}'));
    assert!(label.ends_with('…'));
}

#[test]
fn variants_sharing_a_code_differ_by_name() {
    let mut intl = layouts(&["us", "us"], Some(0));
    intl.layouts[1].name = "English (intl)".to_owned();
    let mut slot = layout(&intl);
    intl.current = Some(1);
    assert!(replace(&mut slot, layout(&intl)));
}
