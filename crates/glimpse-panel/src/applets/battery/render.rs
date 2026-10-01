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
        facts.push(Fact::new(gettext("Make"), vendor.clone()));
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
        DeviceKind::Computer => "computer-symbolic",
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
mod tests;
