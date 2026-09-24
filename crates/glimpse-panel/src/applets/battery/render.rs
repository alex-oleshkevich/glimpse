use gettextrs::gettext;
use glimpse_config::BatteryIndicatorStyle;
use glimpse_dbus::upower::{ChargeState, DeviceKind, Technology, WarningLevel};
use glimpse_services::{BatteryState, BatterySupply, Charge, Profiles};
use glimpse_widgets::{BatteryChargeLimit, BatteryDevice, Choice, Fact, IndicatorSpec, Severity};

use crate::applets::tokens;

const HEALTHY: u8 = 80;
const LOW: u8 = 20;

pub fn chip(
    state: &BatteryState,
    style: BatteryIndicatorStyle,
    format: &str,
    tooltip_format: Option<&str>,
) -> Option<IndicatorSpec> {
    let charge = state.display.as_ref()?;
    let label = match style {
        BatteryIndicatorStyle::IconOnly => None,
        BatteryIndicatorStyle::IconText | BatteryIndicatorStyle::Text => {
            trimmed(&fill(format, charge))
        }
    };
    let (severity, attention) = warning(charge.warning);
    Some(IndicatorSpec {
        icon: None,
        label,
        tooltip: Some(tooltip(charge, tooltip_format)),
        severity,
        attention,
        ..Default::default()
    })
}

pub fn shows_icon(style: BatteryIndicatorStyle) -> bool {
    !matches!(style, BatteryIndicatorStyle::Text)
}

pub struct Heading {
    pub icon: String,
    pub subtitle: Option<String>,
    pub percentage: u8,
    pub severity: Option<Severity>,
}

pub fn heading(state: &BatteryState, full_at: Option<&str>) -> Option<Heading> {
    let charge = state.display.as_ref()?;
    let held_at = state
        .internals
        .first()
        .and_then(|supply| supply.charge_threshold)
        .filter(|threshold| threshold.enabled)
        .and_then(|threshold| threshold.end);
    Some(Heading {
        icon: charge.icon_name(),
        subtitle: subtitle(charge, held_at, full_at),
        percentage: charge.percentage,
        severity: warning(charge.warning).0,
    })
}

pub fn health(supply: &BatterySupply) -> Option<(String, bool)> {
    supply
        .capacity_pct
        .map(|health| (format!("{health}%"), health < HEALTHY))
}

pub fn profiles(profile: Option<&Profiles>) -> (Vec<Choice>, Option<u32>) {
    let Some(profile) = profile else {
        return (Vec::new(), None);
    };
    let choices = profile
        .available
        .iter()
        .map(|name| Choice {
            label: profile_label(name),
            detail: profile_detail(name, held_back(profile)),
            icon_name: profile_icon(name).to_owned(),
            warning: name == "performance" && held_back(profile).is_some(),
        })
        .collect();
    let selected = profile
        .available
        .iter()
        .position(|name| name == &profile.active)
        .map(|index| index as u32);
    (choices, selected)
}

pub fn devices(state: &BatteryState) -> Vec<BatteryDevice> {
    let mut rows: Vec<BatteryDevice> = state
        .devices
        .iter()
        .map(|device| BatteryDevice {
            name: device.name.clone(),
            subtitle: device_state(&device.charge),
            icon_name: device
                .icon_name
                .clone()
                .unwrap_or_else(|| kind_icon(device.kind).to_owned()),
            value: format!("{}%", device.charge.percentage),
            warning: running_low(&device.charge),
        })
        .collect();
    rows.extend(state.internals.iter().skip(1).map(|supply| {
        BatteryDevice {
            name: supply
                .model
                .clone()
                .or_else(|| supply.vendor.clone())
                .unwrap_or_else(|| gettext("Battery")),
            subtitle: device_state(&supply.charge),
            icon_name: if supply.charge.icon_name.is_empty() {
                "battery-symbolic".to_owned()
            } else {
                supply.charge.icon_name.clone()
            },
            value: format!("{}%", supply.charge.percentage),
            warning: running_low(&supply.charge),
        }
    }));
    rows
}

pub fn facts(supply: &BatterySupply) -> Vec<Fact> {
    let mut facts = Vec::new();
    if let (Some(now), Some(full)) = (supply.energy_mwh, supply.energy_full_mwh) {
        facts.push(Fact::new(
            gettext("Energy"),
            format!("{} / {} Wh", tenths(now), tenths(full)),
        ));
    }
    if let Some(design) = supply.energy_full_design_mwh {
        facts.push(Fact::new(
            gettext("Capacity when new"),
            format!("{} Wh", tenths(design)),
        ));
    }
    if let Some(cycles) = supply.cycles {
        facts.push(Fact::new(gettext("Cycles"), cycles.to_string()));
    }
    if let Some(voltage) = supply.voltage_mv {
        facts.push(Fact::new(
            gettext("Voltage"),
            format!("{} V", tenths(voltage)),
        ));
    }
    if let Some(technology) = technology_label(supply.technology) {
        facts.push(Fact::new(gettext("Technology"), technology));
    }
    if let Some(model) = &supply.model {
        facts.push(Fact::new(gettext("Model"), model.clone()));
    }
    if let Some(vendor) = &supply.vendor {
        facts.push(Fact::new(gettext("Vendor"), vendor.clone()));
    }
    facts
}

pub fn charge_limit(supply: &BatterySupply) -> Option<BatteryChargeLimit> {
    let threshold = supply.charge_threshold?;
    let percent = threshold.end?;
    Some(BatteryChargeLimit {
        enabled: threshold.enabled,
        title: gettext("Limit charge to {percent}%").replace("{percent}", &percent.to_string()),
        subtitle: gettext("Slows battery wear"),
    })
}

fn tooltip(charge: &Charge, format: Option<&str>) -> String {
    match format {
        Some(format) => fill(format, charge),
        None => fill("{state} · {percentage}", charge),
    }
}

fn fill(format: &str, charge: &Charge) -> String {
    let percentage = format!("{}%", charge.percentage);
    let state = state_label(charge.state);
    let remaining = remaining(charge).unwrap_or_default();
    tokens::render(format, |token| match token {
        "percentage" => Some(percentage.as_str()),
        "state" => Some(state.as_str()),
        "remaining" => Some(remaining.as_str()),
        _ => None,
    })
}

fn trimmed(rendered: &str) -> Option<String> {
    let text = rendered.trim();
    text.chars()
        .any(char::is_alphanumeric)
        .then(|| text.to_owned())
}

fn subtitle(charge: &Charge, held_at: Option<u32>, full_at: Option<&str>) -> Option<String> {
    match (charge.state, held_at) {
        (ChargeState::Charging, _) => Some(match full_at {
            Some(time) => gettext("Full at {time}").replace("{time}", time),
            None => gettext("Charging"),
        }),
        (ChargeState::PendingCharge | ChargeState::PendingDischarge, Some(percent)) => {
            Some(gettext("Held at {percent}%").replace("{percent}", &percent.to_string()))
        }
        (ChargeState::Discharging, _) => remaining(charge).or_else(|| Some(gettext("Discharging"))),
        (state, _) => Some(state_label(state)).filter(|label| !label.is_empty()),
    }
}

fn device_state(charge: &Charge) -> String {
    match charge.state {
        ChargeState::Charging => gettext("Charging"),
        _ => String::new(),
    }
}

fn running_low(charge: &Charge) -> bool {
    charge.percentage <= LOW && charge.state != ChargeState::Charging
}

fn held_back(profile: &Profiles) -> Option<&str> {
    profile
        .performance_degraded
        .as_deref()
        .filter(|reason| matches!(*reason, "lap-detected" | "high-operating-temperature"))
}

fn remaining(charge: &Charge) -> Option<String> {
    match charge.state {
        ChargeState::Charging | ChargeState::PendingDischarge => charge
            .time_to_full
            .map(|seconds| gettext("{time} to full").replace("{time}", &clock(seconds))),
        ChargeState::Discharging | ChargeState::PendingCharge | ChargeState::Empty => charge
            .time_to_empty
            .map(|seconds| gettext("{time} left").replace("{time}", &clock(seconds))),
        ChargeState::Full | ChargeState::Unknown => None,
    }
}

fn clock(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    match (hours, minutes) {
        (0, 0) => gettext("Less than a minute"),
        (0, minutes) => gettext("{minutes} m").replace("{minutes}", &minutes.to_string()),
        (hours, 0) => gettext("{hours} h").replace("{hours}", &hours.to_string()),
        (hours, minutes) => gettext("{hours} h {minutes} m")
            .replace("{hours}", &hours.to_string())
            .replace("{minutes}", &minutes.to_string()),
    }
}

fn state_label(state: ChargeState) -> String {
    match state {
        ChargeState::Charging => gettext("Charging"),
        ChargeState::Discharging => gettext("Discharging"),
        ChargeState::Empty => gettext("Empty"),
        ChargeState::Full => gettext("Fully charged"),
        ChargeState::PendingCharge | ChargeState::PendingDischarge => gettext("Not charging"),
        ChargeState::Unknown => String::new(),
    }
}

fn warning(level: WarningLevel) -> (Option<Severity>, bool) {
    match level {
        WarningLevel::Low => (Some(Severity::Warning), false),
        WarningLevel::Critical | WarningLevel::Action => (Some(Severity::Error), true),
        WarningLevel::Unknown | WarningLevel::None | WarningLevel::Discharging => (None, false),
    }
}

fn profile_label(name: &str) -> String {
    match name {
        "power-saver" => gettext("Power saver"),
        "balanced" => gettext("Balanced"),
        "performance" => gettext("Performance"),
        other => other.to_owned(),
    }
}

fn profile_detail(name: &str, degraded: Option<&str>) -> String {
    match name {
        "power-saver" => gettext("Longer battery life, slower response"),
        "performance" => match degraded {
            Some("lap-detected") => gettext("Held back — on a lap"),
            Some("high-operating-temperature") => gettext("Held back — too hot"),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

fn profile_icon(name: &str) -> &'static str {
    match name {
        "power-saver" => "power-profile-power-saver-symbolic",
        "performance" => "power-profile-performance-symbolic",
        _ => "power-profile-balanced-symbolic",
    }
}

fn kind_icon(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Mouse => "input-mouse-symbolic",
        DeviceKind::Keyboard => "input-keyboard-symbolic",
        DeviceKind::Headphones | DeviceKind::Headset => "audio-headphones-symbolic",
        DeviceKind::Phone => "phone-symbolic",
        DeviceKind::Tablet => "tablet-symbolic",
        DeviceKind::GamingInput => "input-gaming-symbolic",
        DeviceKind::Ups => "uninterruptible-power-supply-symbolic",
        _ => "battery-symbolic",
    }
}

fn technology_label(technology: Technology) -> Option<String> {
    Some(match technology {
        Technology::LithiumIon => gettext("Li-ion"),
        Technology::LithiumPolymer => gettext("Li-poly"),
        Technology::LithiumIronPhosphate => gettext("LiFePO4"),
        Technology::LeadAcid => gettext("Lead acid"),
        Technology::NickelCadmium => gettext("NiCd"),
        Technology::NickelMetalHydride => gettext("NiMH"),
        Technology::Unknown => return None,
    })
}

fn tenths(milli: u32) -> String {
    format!("{:.1}", f64::from(milli) / 1000.0)
}

#[cfg(test)]
mod tests {
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
        assert!(labels.contains(&"Vendor".to_owned()));
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
}
