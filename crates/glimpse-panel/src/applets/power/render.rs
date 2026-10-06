use chrono::{DateTime, Duration, Local, TimeZone, Timelike, Utc};
use gettextrs::gettext;
use glimpse_config::BatteryIndicatorStyle;
use glimpse_dbus::idle::{HealthKind, IdleProviderState};
use glimpse_dbus::upower::{ChargeState, DeviceKind, Technology, WarningLevel};
use glimpse_services::{BatteryHistoryPoint, BatteryState, BatterySupply, Charge, Profiles};
use glimpse_widgets::{
    Choice, Fact, IndicatorSpec, PowerChargeLimit, PowerDevice, PowerHistory, PowerHoldNotice,
    Severity, Tick,
};

use crate::applets::idle::keep_awake::{awake_until, manual_holds};
use crate::applets::idle::render::holds_idle_or_sleep;
use crate::applets::tokens;

const HEALTHY: u8 = 80;
const LOW: u8 = 20;

pub fn chip(
    state: &BatteryState,
    style: BatteryIndicatorStyle,
    format: &str,
    tooltip_format: Option<&str>,
) -> IndicatorSpec {
    let Some(charge) = state.display.as_ref() else {
        return IndicatorSpec {
            tooltip: Some(gettext("Power")),
            ..Default::default()
        };
    };
    let label = match style {
        BatteryIndicatorStyle::IconOnly => None,
        BatteryIndicatorStyle::IconText | BatteryIndicatorStyle::Text => {
            trimmed(&fill(format, charge))
        }
    };
    let (severity, attention) = warning(charge.warning);
    IndicatorSpec {
        icon: None,
        label,
        tooltip: Some(tooltip(charge, tooltip_format)),
        severity,
        attention,
        ..Default::default()
    }
}

pub fn shows_icon(style: BatteryIndicatorStyle) -> bool {
    !matches!(style, BatteryIndicatorStyle::Text)
}

pub fn header_icon(state: &BatteryState) -> String {
    if let Some(charge) = &state.display {
        return charge.icon_name();
    }
    match &state.profile {
        Some(profile) => format!("power-profile-{}-symbolic", profile.active),
        None => "ac-adapter-symbolic".to_owned(),
    }
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

pub fn devices(state: &BatteryState) -> Vec<PowerDevice> {
    let mut rows: Vec<PowerDevice> = state
        .devices
        .iter()
        .map(|device| PowerDevice {
            name: glimpse_utils::clean(&device.name, 80),
            icon_name: device
                .icon_name
                .clone()
                .unwrap_or_else(|| kind_icon(device.kind).to_owned()),
            value: format!("{}%", device.charge.percentage),
            warning: running_low(&device.charge),
        })
        .collect();
    rows.extend(state.internals.iter().skip(1).map(|supply| {
        PowerDevice {
            name: glimpse_utils::clean(
                &supply
                    .model
                    .clone()
                    .or_else(|| supply.vendor.clone())
                    .unwrap_or_else(|| gettext("Battery")),
                80,
            ),
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

pub fn battery_facts(supply: &BatterySupply, charge: &Charge) -> Vec<Fact> {
    let mut facts = Vec::new();
    if let Some(rate) = charge.energy_rate_mw.filter(|rate| *rate > 0) {
        facts.push(Fact::new(
            gettext("Power draw"),
            gettext("{watts} W").replace("{watts}", &tenths(rate)),
        ));
    }
    if let Some((value, warning)) = health(supply) {
        facts.push(Fact {
            label: gettext("Health"),
            value,
            warning,
        });
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
    if let Some(cycles) = supply.cycles {
        facts.push(Fact::new(gettext("Cycles"), cycles.to_string()));
    }
    if let Some(technology) = technology_label(supply.technology) {
        facts.push(Fact::new(gettext("Technology"), technology));
    }
    if let Some(model) = &supply.model {
        facts.push(Fact::new(gettext("Model"), glimpse_utils::clean(model, 80)));
    }
    if let Some(vendor) = &supply.vendor {
        facts.push(Fact::new(gettext("Make"), glimpse_utils::clean(vendor, 80)));
    }
    facts
}

pub fn charge_limit(supply: &BatterySupply) -> Option<PowerChargeLimit> {
    let threshold = supply.charge_threshold?;
    let percent = threshold.end?;
    Some(PowerChargeLimit {
        enabled: threshold.enabled,
        title: gettext("Limit charge to {percent}%").replace("{percent}", &percent.to_string()),
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
    let remaining = remaining(charge)
        .map(|(_, value)| value)
        .unwrap_or_default();
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

fn running_low(charge: &Charge) -> bool {
    charge.percentage <= LOW && charge.state != ChargeState::Charging
}

fn held_back(profile: &Profiles) -> Option<&str> {
    profile
        .performance_degraded
        .as_deref()
        .filter(|reason| matches!(*reason, "lap-detected" | "high-operating-temperature"))
}

pub fn remaining(charge: &Charge) -> Option<(String, String)> {
    match charge.state {
        ChargeState::Charging => charge
            .time_to_full
            .map(|seconds| (gettext("Until full"), clock(seconds))),
        ChargeState::Discharging => charge
            .time_to_empty
            .map(|seconds| (gettext("Remaining"), clock(seconds))),
        _ => None,
    }
}

pub fn since_unplugged(charge: &Charge, seconds: Option<u32>) -> Option<Fact> {
    if charge.state != ChargeState::Discharging {
        return None;
    }
    seconds.map(|seconds| Fact::new(gettext("Since unplugged"), clock(seconds)))
}

pub fn unplugged_at(
    state: Option<ChargeState>,
    seen_plugged: &mut bool,
    previous: Option<DateTime<Local>>,
    now: DateTime<Local>,
) -> Option<DateTime<Local>> {
    match state {
        Some(ChargeState::Charging | ChargeState::PendingCharge | ChargeState::Full) => {
            *seen_plugged = true;
            None
        }
        Some(ChargeState::Discharging) => previous.or_else(|| seen_plugged.then_some(now)),
        _ => previous,
    }
}

pub fn keep_awake_available(state: &IdleProviderState) -> bool {
    state.available && state.health.login1.kind == HealthKind::Ready
}

pub fn history_minute<Tz: TimeZone>(now: DateTime<Tz>) -> DateTime<Tz> {
    now.clone()
        - Duration::seconds(now.timestamp().rem_euclid(60))
        - Duration::nanoseconds(i64::from(now.timestamp_subsec_nanos()))
}

pub fn history_view<Tz: TimeZone>(
    points: &[BatteryHistoryPoint],
    now: DateTime<Tz>,
    twelve: bool,
) -> PowerHistory
where
    Tz::Offset: std::fmt::Display,
{
    let start = now.clone() - Duration::hours(24);
    let mut sorted: Vec<&BatteryHistoryPoint> = points.iter().collect();
    sorted.sort_by_key(|point| point.at);
    let mut pending = sorted.into_iter().peekable();
    let mut carried = None;
    let slots: Vec<(i64, Option<&BatteryHistoryPoint>)> = (0..96)
        .map(|slot| {
            let begin = start.timestamp() + slot * 900;
            let within = |point: &&BatteryHistoryPoint| {
                let at = i64::from(point.at);
                at < begin + 900 || (slot == 95 && at <= now.timestamp())
            };
            while let Some(point) = pending.next_if(within) {
                carried = Some(point);
            }
            (begin, carried)
        })
        .collect();
    let hour = now
        .with_minute(0)
        .and_then(|at| at.with_second(0))
        .and_then(|at| at.with_nanosecond(0))
        .unwrap_or_else(|| now.clone());
    let mut ticks: Vec<_> = (0..=24)
        .rev()
        .filter_map(|hours| {
            let at = hour.clone() - Duration::hours(hours);
            (at >= start
                && now.clone().signed_duration_since(&at) > Duration::minutes(2)
                && at.hour().is_multiple_of(6)
                && at.minute() == 0
                && at.second() == 0)
                .then(|| Tick {
                    at: (at.timestamp() - start.timestamp()) as f64 / 86_400.0,
                    label: at.format(glimpse_config::clock(twelve)).to_string(),
                })
        })
        .collect();
    ticks.push(Tick {
        at: 1.0,
        label: gettext("Now"),
    });
    PowerHistory {
        values: slots
            .iter()
            .map(|(_, point)| point.map(|point| point.percent))
            .collect(),
        marked: slots
            .iter()
            .map(|(_, point)| point.is_some_and(|point| point.charging))
            .collect(),
        ticks,
        tooltips: slots
            .iter()
            .map(|(begin, point)| {
                let point = (*point)?;
                let at = Utc.timestamp_opt(*begin, 0).single()?;
                Some(
                    gettext("{time} · {percent}%")
                        .replace(
                            "{time}",
                            &at.with_timezone(&now.timezone())
                                .format(glimpse_config::clock(twelve))
                                .to_string(),
                        )
                        .replace("{percent}", &format!("{:.0}", point.percent)),
                )
            })
            .collect(),
    }
}

pub fn hold_notice(state: &IdleProviderState, until: Option<&str>) -> Option<PowerHoldNotice> {
    if !state.inhibitors.iter().any(holds_idle_or_sleep) {
        return None;
    }
    let manual = manual_holds(&state.inhibitors);
    let app = state
        .inhibitors
        .iter()
        .find(|record| holds_idle_or_sleep(record) && !manual.contains(record))
        .map(|record| glimpse_utils::clean(&crate::applets::idle::render::row_label(record), 80));
    Some(PowerHoldNotice {
        title: match until.filter(|_| {
            state
                .inhibitors
                .iter()
                .any(|record| holds_idle_or_sleep(record) && manual.contains(&record))
        }) {
            Some(time) => awake_until(time),
            None => gettext("Kept awake"),
        },
        subtitle: match app {
            Some(app) => gettext("Held by {app}").replace("{app}", &app),
            None => gettext("Held by you"),
        },
    })
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
        "performance" => match degraded {
            Some("lap-detected") => gettext("Unavailable: laptop on lap"),
            Some("high-operating-temperature") => gettext("Unavailable: too hot"),
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
