use glimpse_dbus::upower::ChargeThreshold;
use glimpse_services::{BatteryPeripheral, BatteryState};

use super::*;

fn charge(percentage: u8, state: ChargeState) -> Charge {
    Charge {
        percentage,
        state,
        icon_name: "battery-full-charged-symbolic".to_owned(),
        time_to_empty: None,
        time_to_full: None,
        energy_rate_mw: None,
        warning: WarningLevel::None,
    }
}

fn state(display: Option<Charge>) -> BatteryState {
    BatteryState {
        display,
        ..BatteryState::default()
    }
}

#[test]
fn no_display_device_shows_no_chip() {
    assert!(
        chip(
            &state(None),
            BatteryIndicatorStyle::IconOnly,
            "{percentage}",
            None,
        )
        .is_none()
    );
}

#[test]
fn icon_only_drops_the_label() {
    let spec = chip(
        &state(Some(charge(100, ChargeState::Full))),
        BatteryIndicatorStyle::IconOnly,
        "{percentage}",
        None,
    )
    .unwrap();
    assert!(spec.label.is_none());
    assert!(shows_icon(BatteryIndicatorStyle::IconOnly));
}

#[test]
fn icon_text_keeps_the_percentage() {
    let spec = chip(
        &state(Some(charge(87, ChargeState::Discharging))),
        BatteryIndicatorStyle::IconText,
        "{percentage}",
        None,
    )
    .unwrap();
    assert_eq!(spec.label.as_deref(), Some("87%"));
    assert!(shows_icon(BatteryIndicatorStyle::IconText));
}

#[test]
fn text_drops_the_icon() {
    let spec = chip(
        &state(Some(charge(87, ChargeState::Discharging))),
        BatteryIndicatorStyle::Text,
        "{percentage}",
        None,
    )
    .unwrap();
    assert_eq!(spec.label.as_deref(), Some("87%"));
    assert!(!shows_icon(BatteryIndicatorStyle::Text));
}

#[test]
fn empty_remaining_is_trimmed_out_of_a_format() {
    let spec = chip(
        &state(Some(charge(100, ChargeState::Full))),
        BatteryIndicatorStyle::IconText,
        "{remaining}",
        None,
    )
    .unwrap();
    assert!(spec.label.is_none());
}

#[test]
fn a_low_battery_is_a_warning_and_a_critical_one_demands_attention() {
    let mut low = charge(15, ChargeState::Discharging);
    low.warning = WarningLevel::Low;
    let spec = chip(
        &state(Some(low)),
        BatteryIndicatorStyle::IconOnly,
        "{percentage}",
        None,
    )
    .unwrap();
    assert_eq!(spec.severity, Some(Severity::Warning));
    assert!(!spec.attention);

    let mut critical = charge(5, ChargeState::Discharging);
    critical.warning = WarningLevel::Critical;
    let spec = chip(
        &state(Some(critical)),
        BatteryIndicatorStyle::IconOnly,
        "{percentage}",
        None,
    )
    .unwrap();
    assert_eq!(spec.severity, Some(Severity::Error));
    assert!(spec.attention);
}

#[test]
fn discharging_remaining_is_time_left() {
    let mut discharging = charge(40, ChargeState::Discharging);
    discharging.time_to_empty = Some(12_000);
    assert_eq!(remaining(&discharging).as_deref(), Some("3 h 20 m left"));
}

#[test]
fn fully_charged_has_no_remaining_and_says_so() {
    assert_eq!(
        subtitle(&charge(100, ChargeState::Full), None, None).as_deref(),
        Some("Fully charged")
    );
    assert!(remaining(&charge(100, ChargeState::Full)).is_none());
}

#[test]
fn the_hero_says_when_rather_than_what() {
    let mut discharging = charge(40, ChargeState::Discharging);
    discharging.time_to_empty = Some(12_000);
    assert_eq!(
        subtitle(&discharging, None, None).as_deref(),
        Some("3 h 20 m left"),
        "draining is implied by time left"
    );
    assert_eq!(
        subtitle(&charge(60, ChargeState::Charging), None, Some("14:30")).as_deref(),
        Some("Full at 14:30")
    );
    assert_eq!(
        subtitle(&charge(80, ChargeState::PendingCharge), Some(80), None).as_deref(),
        Some("Held at 80%"),
        "a limit holding the charge says so rather than \"Not charging\""
    );
    assert_eq!(
        subtitle(&charge(55, ChargeState::PendingCharge), None, None).as_deref(),
        Some("Not charging")
    );
}

#[test]
fn the_hero_carries_the_chip_severity() {
    let mut low = charge(15, ChargeState::Discharging);
    low.warning = WarningLevel::Low;
    let shown = heading(&state(Some(low)), None).unwrap();
    assert_eq!(shown.severity, Some(Severity::Warning));
}

#[test]
fn details_omit_cycles_and_rate_when_the_pack_is_full() {
    let mut supply = BatterySupply {
        path: "/bat1".to_owned(),
        charge: charge(100, ChargeState::Full),
        vendor: Some("ASUS".to_owned()),
        model: Some("A32-K55".to_owned()),
        native_path: Some("BAT1".to_owned()),
        energy_mwh: Some(87_042),
        energy_full_mwh: Some(87_042),
        energy_full_design_mwh: Some(90_005),
        capacity_pct: Some(97),
        voltage_mv: Some(17_243),
        cycles: None,
        technology: Technology::LithiumIon,
        charge_threshold: Some(ChargeThreshold {
            enabled: false,
            start: Some(75),
            end: Some(80),
        }),
    };
    let labels: Vec<_> = facts(&supply).into_iter().map(|fact| fact.label).collect();
    assert!(labels.contains(&"Make".to_owned()));
    assert!(!labels.iter().any(|label| label == "Cycles"));
    assert!(
        !labels
            .iter()
            .any(|label| ["Charge", "Health", "Rate", "Time left"].contains(&label.as_str())),
        "the hero and the health row already say these"
    );
    assert_eq!(health(&supply), Some(("97%".to_owned(), false)));
    supply.capacity_pct = Some(74);
    assert_eq!(health(&supply), Some(("74%".to_owned(), true)));
    assert_eq!(charge_limit(&supply).unwrap().title, "Limit charge to 80%");
}

#[test]
fn an_unspecified_end_hides_the_charge_limit() {
    let supply = BatterySupply {
        path: "/bat1".to_owned(),
        charge: charge(100, ChargeState::Full),
        vendor: None,
        model: None,
        native_path: None,
        energy_mwh: None,
        energy_full_mwh: None,
        energy_full_design_mwh: None,
        capacity_pct: None,
        voltage_mv: None,
        cycles: None,
        technology: Technology::Unknown,
        charge_threshold: Some(ChargeThreshold {
            enabled: false,
            start: None,
            end: None,
        }),
    };
    assert!(charge_limit(&supply).is_none());
}

#[test]
fn a_device_row_says_charging_and_reads_amber_when_low() {
    let rows = devices(&BatteryState {
        devices: vec![BatteryPeripheral {
            path: "/mouse".to_owned(),
            kind: DeviceKind::Mouse,
            name: "MX Master 3S".to_owned(),
            icon_name: None,
            charge: charge(41, ChargeState::Discharging),
        }],
        ..BatteryState::default()
    });
    assert_eq!(rows[0].subtitle, "", "the icon already says mouse");
    assert_eq!(rows[0].value, "41%");
    assert!(rows[0].icon_name.contains("mouse"));
    assert!(!rows[0].warning);

    let rows = devices(&BatteryState {
        devices: vec![
            BatteryPeripheral {
                path: "/mouse".to_owned(),
                kind: DeviceKind::Mouse,
                name: "MX Master 3S".to_owned(),
                icon_name: None,
                charge: charge(12, ChargeState::Discharging),
            },
            BatteryPeripheral {
                path: "/headset".to_owned(),
                kind: DeviceKind::Headset,
                name: "WH-1000XM4".to_owned(),
                icon_name: None,
                charge: charge(12, ChargeState::Charging),
            },
        ],
        ..BatteryState::default()
    });
    assert!(rows[0].warning);
    assert!(
        !rows[1].warning,
        "a device on its charger is not running low"
    );
    assert_eq!(rows[1].subtitle, "Charging");
}

#[test]
fn a_second_internal_pack_shows_as_a_device_row() {
    let primary = BatterySupply {
        path: "/bat0".to_owned(),
        charge: charge(100, ChargeState::Full),
        vendor: None,
        model: Some("BAT0".to_owned()),
        native_path: None,
        energy_mwh: None,
        energy_full_mwh: None,
        energy_full_design_mwh: None,
        capacity_pct: None,
        voltage_mv: None,
        cycles: None,
        technology: Technology::Unknown,
        charge_threshold: None,
    };
    let extra = BatterySupply {
        path: "/bat1".to_owned(),
        charge: charge(40, ChargeState::Discharging),
        vendor: None,
        model: Some("BAT1".to_owned()),
        native_path: None,
        energy_mwh: None,
        energy_full_mwh: None,
        energy_full_design_mwh: None,
        capacity_pct: None,
        voltage_mv: None,
        cycles: None,
        technology: Technology::Unknown,
        charge_threshold: None,
    };
    let rows = devices(&BatteryState {
        internals: vec![primary, extra],
        ..BatteryState::default()
    });
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "BAT1");
    assert_eq!(rows[0].value, "40%");
}

#[test]
fn power_mode_selects_the_active_profile() {
    let (choices, selected) = profiles(Some(&Profiles {
        active: "balanced".to_owned(),
        available: vec![
            "power-saver".to_owned(),
            "balanced".to_owned(),
            "performance".to_owned(),
        ],
        performance_degraded: None,
    }));
    assert_eq!(choices.len(), 3);
    assert_eq!(selected, Some(1));
    assert_eq!(choices[0].label, "Power saver");
    let (held, _) = profiles(Some(&Profiles {
        active: "performance".to_owned(),
        available: vec!["performance".to_owned()],
        performance_degraded: Some("lap-detected".to_owned()),
    }));
    assert_eq!(held[0].detail, "Held back — on a lap");
    let (unknown, _) = profiles(Some(&Profiles {
        active: "performance".to_owned(),
        available: vec!["performance".to_owned()],
        performance_degraded: Some("something-else".to_owned()),
    }));
    assert!(unknown[0].detail.is_empty());
    assert!(held[0].warning, "a held-back performance mode reads amber");
    assert!(!unknown[0].warning);
    assert!(
        choices[1].detail.is_empty(),
        "balanced has nothing to explain"
    );
}
