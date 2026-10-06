use glimpse_dbus::idle::{
    IdleInhibitorRecord, IdleInhibitorSource, IdleProvider, InhibitionTargets, Login1Mode,
};
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
fn no_display_device_keeps_a_chip_without_percentage() {
    for style in [
        BatteryIndicatorStyle::IconOnly,
        BatteryIndicatorStyle::IconText,
        BatteryIndicatorStyle::Text,
    ] {
        let spec = chip(&state(None), style, "{percentage}", None);
        assert!(spec.label.is_none());
        assert_eq!(spec.tooltip.as_deref(), Some("Power"));
    }
}

#[test]
fn icon_only_drops_the_label() {
    let spec = chip(
        &state(Some(charge(100, ChargeState::Full))),
        BatteryIndicatorStyle::IconOnly,
        "{percentage}",
        None,
    );
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
    );
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
    );
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
    );
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
    );
    assert_eq!(spec.severity, Some(Severity::Warning));
    assert!(!spec.attention);

    let mut critical = charge(5, ChargeState::Discharging);
    critical.warning = WarningLevel::Critical;
    let spec = chip(
        &state(Some(critical)),
        BatteryIndicatorStyle::IconOnly,
        "{percentage}",
        None,
    );
    assert_eq!(spec.severity, Some(Severity::Error));
    assert!(spec.attention);
}

#[test]
fn remaining_titles_and_hidden_states() {
    let mut discharging = charge(40, ChargeState::Discharging);
    discharging.time_to_empty = Some(12_000);
    assert_eq!(
        remaining(&discharging),
        Some(("Remaining".to_owned(), "3 h 20 m".to_owned()))
    );
    let mut charging = charge(60, ChargeState::Charging);
    charging.time_to_full = Some(3600);
    assert_eq!(
        remaining(&charging),
        Some(("Until full".to_owned(), "1 h".to_owned()))
    );
    for state in [
        ChargeState::Full,
        ChargeState::Unknown,
        ChargeState::Empty,
        ChargeState::PendingCharge,
        ChargeState::PendingDischarge,
    ] {
        let mut hidden = charge(50, state);
        hidden.time_to_full = Some(3600);
        hidden.time_to_empty = Some(3600);
        assert!(remaining(&hidden).is_none());
    }
    for state in [ChargeState::Charging, ChargeState::Discharging] {
        assert!(remaining(&charge(50, state)).is_none());
    }
}

#[test]
fn header_icon_follows_battery_then_profile_then_ac() {
    let mut laptop = state(Some(charge(60, ChargeState::Charging)));
    laptop.profile = Some(Profiles {
        active: "performance".to_owned(),
        available: vec!["performance".to_owned()],
        performance_degraded: None,
    });
    assert_eq!(header_icon(&laptop), "battery-full-charged-symbolic");
    laptop.display = None;
    assert_eq!(header_icon(&laptop), "power-profile-performance-symbolic");
    laptop.profile = None;
    assert_eq!(header_icon(&laptop), "ac-adapter-symbolic");
}

#[test]
fn battery_facts_keep_the_order_and_health_threshold() {
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
    let labels: Vec<_> = battery_facts(&supply, &supply.charge)
        .into_iter()
        .map(|fact| fact.label)
        .collect();
    assert_eq!(
        labels,
        [
            "Health",
            "Energy",
            "Capacity when new",
            "Technology",
            "Model",
            "Make"
        ]
    );
    assert_eq!(health(&supply), Some(("97%".to_owned(), false)));
    supply.capacity_pct = Some(74);
    assert_eq!(health(&supply), Some(("74%".to_owned(), true)));
    assert!(battery_facts(&supply, &supply.charge)[0].warning);
    supply.capacity_pct = Some(80);
    assert_eq!(health(&supply), Some(("80%".to_owned(), false)));
    assert_eq!(charge_limit(&supply).unwrap().title, "Limit charge to 80%");
    supply.charge.energy_rate_mw = Some(12_345);
    supply.cycles = Some(200);
    let facts = battery_facts(&supply, &supply.charge);
    assert_eq!(facts[0].label, "Power draw");
    assert_eq!(facts[0].value, "12.3 W");
    assert_eq!(
        facts
            .iter()
            .map(|fact| fact.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Power draw",
            "Health",
            "Energy",
            "Capacity when new",
            "Cycles",
            "Technology",
            "Model",
            "Make"
        ]
    );
    for rate in [Some(0), None] {
        supply.charge.energy_rate_mw = rate;
        assert!(
            !battery_facts(&supply, &supply.charge)
                .iter()
                .any(|fact| fact.label == "Power draw")
        );
    }
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
    assert!(battery_facts(&supply, &supply.charge).is_empty());
    let supply = BatterySupply {
        charge_threshold: None,
        energy_mwh: Some(10_000),
        ..supply
    };
    assert!(charge_limit(&supply).is_none());
    assert!(battery_facts(&supply, &supply.charge).is_empty());
}

#[test]
fn a_device_row_reads_amber_when_low_unless_charging() {
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
    let (missing, selected) = profiles(None);
    assert!(missing.is_empty());
    assert!(selected.is_none());
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
    assert!(choices[0].detail.is_empty());
    let (held, _) = profiles(Some(&Profiles {
        active: "performance".to_owned(),
        available: vec!["performance".to_owned()],
        performance_degraded: Some("lap-detected".to_owned()),
    }));
    assert_eq!(held[0].detail, "Unavailable: laptop on lap");
    let (unknown, _) = profiles(Some(&Profiles {
        active: "performance".to_owned(),
        available: vec!["performance".to_owned()],
        performance_degraded: Some("something-else".to_owned()),
    }));
    assert!(unknown[0].detail.is_empty());
    assert!(held[0].warning, "a held-back performance mode reads amber");
    assert!(!unknown[0].warning);
    let (hot, _) = profiles(Some(&Profiles {
        active: "performance".to_owned(),
        available: vec!["performance".to_owned()],
        performance_degraded: Some("high-operating-temperature".to_owned()),
    }));
    assert_eq!(hot[0].detail, "Unavailable: too hot");
    assert!(hot[0].warning);
    assert!(
        choices[1].detail.is_empty(),
        "balanced has nothing to explain"
    );
}

#[test]
fn since_unplugged_only_appears_while_discharging() {
    let fact = since_unplugged(&charge(40, ChargeState::Discharging), Some(12_000));
    assert_eq!(
        fact.as_ref()
            .map(|fact| (fact.label.as_str(), fact.value.as_str())),
        Some(("Since unplugged", "3 h 20 m"))
    );
    assert!(since_unplugged(&charge(40, ChargeState::Discharging), None).is_none());
    for state in [
        ChargeState::Charging,
        ChargeState::Full,
        ChargeState::Unknown,
        ChargeState::Empty,
        ChargeState::PendingCharge,
        ChargeState::PendingDischarge,
    ] {
        assert!(since_unplugged(&charge(50, state), Some(3600)).is_none());
    }
}

#[test]
fn history_uses_local_six_hour_ticks_and_marks_charging() {
    let Some(now) = Local.with_ymd_and_hms(2026, 1, 15, 13, 20, 0).single() else {
        panic!("fixture is an unambiguous local time");
    };
    let point = |hours, percent, charging| BatteryHistoryPoint {
        at: (now - Duration::hours(hours)).timestamp() as u32,
        percent,
        charging,
    };
    let points = [
        point(1, 60.4, true),
        point(25, 10.0, false),
        point(20, 85.0, false),
        point(-1, 90.0, true),
    ];
    let history = history_view(&points, now, false);
    let mut values = vec![Some(10.0); 96];
    values[16..92].fill(Some(85.0));
    values[92..].fill(Some(60.4));
    assert_eq!(history.values, values);
    let mut marked = vec![false; 96];
    marked[92..].fill(true);
    assert_eq!(history.marked, marked);
    let tooltip = |time: &str, percent: &str| {
        Some(
            gettext("{time} · {percent}%")
                .replace("{time}", time)
                .replace("{percent}", percent),
        )
    };
    assert_eq!(history.tooltips[0], tooltip("13:20", "10"));
    assert_eq!(history.tooltips[16], tooltip("17:20", "85"));
    assert_eq!(history.tooltips[92], tooltip("12:20", "60"));
    assert_eq!(
        history
            .ticks
            .iter()
            .map(|tick| tick.label.as_str())
            .collect::<Vec<_>>(),
        ["18:00", "00:00", "06:00", "12:00", &gettext("Now")]
    );
    for (tick, minutes) in history.ticks.iter().zip([280, 640, 1000, 1360, 1440]) {
        assert!((tick.at - f64::from(minutes) / 1440.0).abs() < 0.000_001);
    }
    let twelve = history_view(&points, now, true);
    for (tick, hours) in twelve.ticks.iter().zip([19, 13, 7, 1]) {
        let at = (now - Duration::hours(hours)).with_minute(0).unwrap();
        assert_eq!(
            tick.label,
            at.format(glimpse_config::clock(true)).to_string()
        );
    }
    assert_eq!(twelve.ticks[4].label, gettext("Now"));
    assert_eq!(
        twelve.tooltips[92],
        tooltip(
            &(now - Duration::hours(1))
                .format(glimpse_config::clock(true))
                .to_string(),
            "60"
        )
    );
}

#[test]
fn history_slots_keep_the_latest_point_and_include_both_window_edges() {
    let now = Local
        .with_ymd_and_hms(2026, 1, 15, 13, 20, 0)
        .single()
        .unwrap();
    let start = now - Duration::hours(24);
    let point = |seconds, percent, charging| BatteryHistoryPoint {
        at: (start + Duration::seconds(seconds)).timestamp() as u32,
        percent,
        charging,
    };
    let points = [
        point(1800, 80.0, false),
        point(899, 75.0, true),
        point(0, 90.0, false),
        point(900, 85.0, false),
        point(86_400, 50.0, true),
        point(86_399, 55.0, false),
        point(-1, 1.0, false),
        point(86_401, 1.0, false),
    ];
    let history = history_view(&points, now, false);
    assert_eq!(history.values.len(), 96);
    assert_eq!(
        &history.values[..4],
        &[Some(75.0), Some(85.0), Some(80.0), Some(80.0)]
    );
    assert!(
        history.values[3..95]
            .iter()
            .all(|value| *value == Some(80.0))
    );
    assert_eq!(history.values[95], Some(50.0));
    assert!(history.marked[0] && history.marked[95]);
    assert!(!history.marked[1]);
    let time = start.format(glimpse_config::clock(false)).to_string();
    assert_eq!(
        history.tooltips[0],
        Some(
            gettext("{time} · {percent}%")
                .replace("{time}", &time)
                .replace("{percent}", "75")
        )
    );
}

#[test]
fn six_hour_ticks_avoid_now_and_half_hour_dst_boundaries() {
    for zone in [chrono_tz::UTC, chrono_tz::Australia::Lord_Howe] {
        let now = zone
            .with_ymd_and_hms(2026, 4, 5, 18, 0, 30)
            .single()
            .unwrap();
        let start = now - Duration::hours(24);
        let history = history_view(&[], now, false);
        assert_eq!(history.ticks.last().unwrap().label, gettext("Now"));
        for tick in &history.ticks[..history.ticks.len() - 1] {
            let at = start + Duration::seconds((tick.at * 86_400.0).round() as i64);
            assert!(now - at > Duration::minutes(2));
            assert!(at.hour().is_multiple_of(6));
            assert_eq!(at.minute(), 0);
            assert_eq!(at.second(), 0);
        }
        assert!(history.ticks.len() >= 3);
    }
}

#[test]
fn unplugged_time_survives_transient_states_until_charging() {
    let start = Local
        .with_ymd_and_hms(2026, 1, 15, 13, 20, 0)
        .single()
        .unwrap();
    let later = start + Duration::hours(2);
    let mut seen_plugged = false;
    assert_eq!(
        unplugged_at(
            Some(ChargeState::Discharging),
            &mut seen_plugged,
            None,
            start
        ),
        None
    );
    for plugged in [
        ChargeState::Charging,
        ChargeState::PendingCharge,
        ChargeState::Full,
    ] {
        assert_eq!(
            unplugged_at(Some(plugged), &mut seen_plugged, None, start),
            None
        );
        assert!(seen_plugged);
        let mut since = None;
        for state in [
            Some(ChargeState::PendingDischarge),
            Some(ChargeState::Unknown),
            None,
        ] {
            since = unplugged_at(state, &mut seen_plugged, since, start);
            assert_eq!(since, None);
        }
        since = unplugged_at(
            Some(ChargeState::Discharging),
            &mut seen_plugged,
            since,
            later,
        );
        assert_eq!(since, Some(later));
        for state in [
            Some(ChargeState::Unknown),
            Some(ChargeState::Empty),
            Some(ChargeState::PendingDischarge),
            None,
            Some(ChargeState::Discharging),
        ] {
            since = unplugged_at(
                state,
                &mut seen_plugged,
                since,
                later + Duration::minutes(1),
            );
            assert_eq!(since, Some(later));
        }
        assert_eq!(
            unplugged_at(Some(plugged), &mut seen_plugged, since, later),
            None
        );
    }
    seen_plugged = false;
    for state in [
        Some(ChargeState::Unknown),
        Some(ChargeState::Empty),
        Some(ChargeState::PendingDischarge),
        None,
        Some(ChargeState::Discharging),
    ] {
        assert_eq!(unplugged_at(state, &mut seen_plugged, None, later), None);
    }
}

#[test]
fn history_with_a_live_point_is_stable_within_a_minute() {
    let start = Utc
        .with_ymd_and_hms(2026, 1, 15, 13, 20, 0)
        .single()
        .unwrap();
    let history = |now| {
        let now = history_minute(now);
        let live = BatteryHistoryPoint {
            at: now.timestamp() as u32,
            percent: 50.0,
            charging: false,
        };
        history_view(&[live], now, false)
    };
    let later = start + Duration::seconds(59) + Duration::nanoseconds(999_999_999);
    assert_eq!(history_minute(later), start);
    assert_eq!(history(later), history(start));
    assert_ne!(history(start + Duration::minutes(1)), history(start));
}

#[test]
fn hold_deadline_leaves_with_the_provider_record() {
    use crate::applets::idle::keep_awake::deadline;

    let mut records = vec![IdleInhibitorRecord {
        added_at_unix: 1_789_382_400,
        ..holder(true)
    }];
    let expiries = std::collections::BTreeMap::from([(1, 1_789_383_300)]);
    let held = deadline(&records, &expiries).unwrap();
    let ends = held.0.format("%H:%M").to_string();
    assert_eq!(held.1, 900);
    let mut state = IdleProvider::unavailable("no bus in tests")
        .handle()
        .snapshot();
    state.inhibitors = records.clone();
    assert_eq!(
        hold_notice(&state, Some(&ends)).unwrap().title,
        awake_until(&ends)
    );

    records.clear();
    assert_eq!(deadline(&records, &expiries), None);
    records.push(holder(false));
    assert_eq!(deadline(&records, &expiries), None);
    records.push(holder(true));
    assert_eq!(deadline(&records, &std::collections::BTreeMap::new()), None);
}

#[test]
fn a_replaced_preset_is_derived_independently_by_both_applets() {
    use crate::applets::idle::keep_awake::deadline;

    let old = IdleInhibitorRecord {
        added_at_unix: 1_789_382_400,
        ..holder(true)
    };
    let new = IdleInhibitorRecord {
        id: old.id + 1,
        added_at_unix: old.added_at_unix + 1,
        ..old.clone()
    };
    let expiries = std::collections::BTreeMap::from([
        (old.id, 1_789_383_300),
        (new.id, new.added_at_unix + 3_600),
    ]);
    let mut power = vec![old.clone()];
    let mut idle = vec![new.clone(), old];
    let replaced = deadline(&idle, &expiries).unwrap();
    assert_eq!(replaced.1, 3_600);
    assert_eq!(deadline(&power, &expiries).unwrap().1, 900);
    assert_eq!(deadline(&idle, &expiries), Some(replaced));

    power.clear();
    assert_eq!(deadline(&power, &expiries), None);
    assert_eq!(deadline(&idle, &expiries), Some(replaced));
    power.push(new.clone());
    idle = vec![new];
    assert_eq!(deadline(&power, &expiries), deadline(&idle, &expiries));
}

#[test]
fn keep_awake_requires_login1_but_tolerates_wayland_degradation() {
    use glimpse_dbus::idle::HealthKind;
    let mut state = IdleProvider::unavailable("no bus in tests")
        .handle()
        .snapshot();
    assert!(!keep_awake_available(&state));
    state.available = true;
    state.owner = true;
    state.reason = None;
    state.health.login1.kind = HealthKind::Ready;
    assert!(keep_awake_available(&state));
    state.health.wayland.kind = HealthKind::Degraded;
    assert!(keep_awake_available(&state));
    for kind in [HealthKind::Unsupported, HealthKind::Degraded] {
        state.health.login1.kind = kind;
        assert!(!keep_awake_available(&state));
    }
}

fn holder(manual: bool) -> IdleInhibitorRecord {
    IdleInhibitorRecord {
        id: if manual { 1 } else { 2 },
        who: if manual { "glimpse-idle" } else { "Firefox" }.to_owned(),
        why: if manual {
            "Manual hold"
        } else {
            "Playing video"
        }
        .to_owned(),
        bus_name: String::new(),
        process_name: String::new(),
        source: IdleInhibitorSource::login1(4242, 1000, Login1Mode::Block),
        targets: InhibitionTargets::manual_hold(),
        can_release: true,
        added_at_unix: 0,
    }
}

#[test]
fn hold_notice_drives_the_warning_chip_and_names_the_holder() {
    let mut state = IdleProvider::unavailable("no bus in tests")
        .handle()
        .snapshot();
    assert!(hold_notice(&state, None).is_none());
    assert!(hold_notice(&state, Some("15:40")).is_none());
    let mut lid_only = holder(false);
    lid_only.targets = InhibitionTargets::from_login1_what("handle-lid-switch");
    state.inhibitors.push(lid_only);
    assert!(hold_notice(&state, None).is_none());
    state.inhibitors.clear();
    state.inhibitors.push(holder(true));
    let notice = hold_notice(&state, Some("15:40"));
    assert_eq!(
        notice
            .as_ref()
            .map(|notice| (notice.title.as_str(), notice.subtitle.as_str())),
        Some(("Awake until 15:40", "Held by you"))
    );
    let notice = hold_notice(&state, None);
    assert_eq!(
        notice.as_ref().map(|notice| notice.title.as_str()),
        Some("Kept awake")
    );
    state.inhibitors.push(holder(false));
    let notice = hold_notice(&state, Some("15:40"));
    assert_eq!(
        notice.as_ref().map(|notice| notice.subtitle.as_str()),
        Some("Held by Firefox")
    );
    state.inhibitors.remove(0);
    let notice = hold_notice(&state, Some("15:40"));
    assert_eq!(
        notice
            .as_ref()
            .map(|notice| (notice.title.as_str(), notice.subtitle.as_str())),
        Some(("Kept awake", "Held by Firefox"))
    );
    let mut ignored_manual = holder(true);
    ignored_manual.targets = InhibitionTargets::from_login1_what("handle-lid-switch");
    state.inhibitors.push(ignored_manual);
    assert_eq!(
        hold_notice(&state, Some("15:40")).unwrap().title,
        gettext("Kept awake")
    );
    state.inhibitors[0].process_name = "x".repeat(10_000);
    let notice = hold_notice(&state, None);
    assert!(
        notice
            .as_ref()
            .is_some_and(|notice| notice.subtitle.chars().count() <= 90)
    );
    state.inhibitors.clear();
    assert!(hold_notice(&state, None).is_none());
}

#[test]
fn history_carries_the_last_reading_across_a_quiet_window() {
    let now = Utc
        .with_ymd_and_hms(2026, 10, 5, 12, 0, 0)
        .single()
        .unwrap();
    let old = (now - Duration::days(6)).timestamp() as u32;
    let points = [
        BatteryHistoryPoint {
            at: old,
            percent: 100.0,
            charging: false,
        },
        BatteryHistoryPoint {
            at: now.timestamp() as u32,
            percent: 100.0,
            charging: false,
        },
    ];
    let history = history_view(&points, now, false);
    assert_eq!(history.values.len(), 96);
    assert!(history.values.iter().all(|value| *value == Some(100.0)));
    assert!(history.tooltips.iter().all(Option::is_some));
}

#[test]
fn history_before_the_first_reading_stays_empty() {
    let now = Utc
        .with_ymd_and_hms(2026, 10, 5, 12, 0, 0)
        .single()
        .unwrap();
    let at = (now - Duration::hours(6)).timestamp() as u32;
    let points = [BatteryHistoryPoint {
        at,
        percent: 50.0,
        charging: true,
    }];
    let history = history_view(&points, now, false);
    assert_eq!(history.values[0], None);
    assert_eq!(history.values[95], Some(50.0));
    assert!(history.marked[95]);
}
