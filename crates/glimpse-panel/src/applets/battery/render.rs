use gettextrs::gettext;
use glimpse_config::BatteryIndicatorStyle;
use glimpse_dbus::upower::{ChargeState, DeviceKind, Technology, WarningLevel};
use glimpse_services::{BatteryState, BatterySupply, Charge, Profiles};
use glimpse_widgets::{BatteryChargeLimit, BatteryDevice, Choice, Fact, IndicatorSpec, Severity};

use crate::applets::tokens;

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

pub fn heading(charge: &Charge) -> (String, Option<String>, u8) {
    (charge.icon_name(), subtitle(charge), charge.percentage)
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
            detail: profile_detail(name, profile.performance_degraded.as_deref()),
            icon_name: profile_icon(name).to_owned(),
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
            subtitle: kind_label(device.kind),
            icon_name: device
                .icon_name
                .clone()
                .unwrap_or_else(|| kind_icon(device.kind).to_owned()),
            value: format!("{}%", device.charge.percentage),
        })
        .collect();
    rows.extend(state.internals.iter().skip(1).map(|supply| {
        BatteryDevice {
            name: supply
                .model
                .clone()
                .or_else(|| supply.vendor.clone())
                .unwrap_or_else(|| gettext("Battery")),
            subtitle: gettext("Battery"),
            icon_name: if supply.charge.icon_name.is_empty() {
                "battery-symbolic".to_owned()
            } else {
                supply.charge.icon_name.clone()
            },
            value: format!("{}%", supply.charge.percentage),
        }
    }));
    rows
}

pub fn facts(supply: &BatterySupply) -> Vec<Fact> {
    let mut facts = vec![Fact::new(
        gettext("Charge"),
        format!("{}%", supply.charge.percentage),
    )];
    if let Some(remaining) = remaining(&supply.charge) {
        let title = match supply.charge.state {
            ChargeState::Charging | ChargeState::PendingDischarge => gettext("Time to full"),
            _ => gettext("Time left"),
        };
        facts.push(Fact::new(title, remaining));
    }
    if let Some(rate) = supply.charge.energy_rate_mw.filter(|_| {
        matches!(
            supply.charge.state,
            ChargeState::Charging | ChargeState::Discharging
        )
    }) {
        facts.push(Fact::new(gettext("Rate"), watts(rate)));
    }
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
    if let Some(health) = supply.capacity_pct {
        facts.push(Fact::new(gettext("Health"), format!("{health}%")));
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
        subtitle: gettext("Stops at {percent}% to slow wear")
            .replace("{percent}", &percent.to_string()),
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

fn subtitle(charge: &Charge) -> Option<String> {
    let state = state_label(charge.state);
    match (state.as_str(), remaining(charge)) {
        ("", None) => None,
        ("", Some(remaining)) => Some(remaining),
        (state, None) => Some(state.to_owned()),
        (state, Some(remaining)) => Some(format!("{state} · {remaining}")),
    }
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
        "balanced" => gettext("The default trade-off"),
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

fn kind_label(kind: DeviceKind) -> String {
    match kind {
        DeviceKind::Mouse => gettext("Mouse"),
        DeviceKind::Keyboard => gettext("Keyboard"),
        DeviceKind::Headphones => gettext("Headphones"),
        DeviceKind::Headset => gettext("Headset"),
        DeviceKind::Phone => gettext("Phone"),
        DeviceKind::Tablet => gettext("Tablet"),
        DeviceKind::GamingInput => gettext("Controller"),
        DeviceKind::Ups => gettext("UPS"),
        _ => gettext("Device"),
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

fn watts(mw: u32) -> String {
    format!("{} W", tenths(mw))
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
            subtitle(&charge(100, ChargeState::Full)).as_deref(),
            Some("Fully charged")
        );
        assert!(remaining(&charge(100, ChargeState::Full)).is_none());
    }

    #[test]
    fn details_omit_cycles_and_rate_when_the_pack_is_full() {
        let supply = BatterySupply {
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
        assert!(labels.contains(&"Charge".to_owned()));
        assert!(labels.contains(&"Health".to_owned()));
        assert!(labels.contains(&"Vendor".to_owned()));
        assert!(!labels.iter().any(|label| label == "Cycles"));
        assert!(!labels.iter().any(|label| label == "Rate"));
        assert_eq!(
            charge_limit(&supply).unwrap().subtitle,
            "Stops at 80% to slow wear"
        );
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
    fn a_mouse_row_uses_its_kind_as_the_subtitle() {
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
        assert_eq!(rows[0].subtitle, "Mouse");
        assert_eq!(rows[0].value, "41%");
        assert!(rows[0].icon_name.contains("mouse"));
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
    }
}
