use glimpse_dbus::upower::WarningLevel;
use glimpse_dbus::weather::{WatchedPlace, WeatherProviderState, reading};
use glimpse_services::{BatteryState, BluetoothState, KeyboardLayouts, NetworkState};
use glimpse_utils::clean;
use glimpse_widgets::{IndicatorSpec, Severity};
use gtk4::gio;
use gtk4::prelude::{Cast, IconExt};

const LAYOUT_CAP: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub weather: Option<IndicatorSpec>,
    pub battery: Option<IndicatorSpec>,
    pub layout: Option<IndicatorSpec>,
    pub bluetooth: Option<IndicatorSpec>,
    pub network: Option<IndicatorSpec>,
}

pub fn shown(status: &Status, enabled: bool) -> Status {
    match enabled {
        true => status.clone(),
        false => Status::default(),
    }
}

pub fn battery(state: &BatteryState) -> Option<IndicatorSpec> {
    let charge = state.display.as_ref()?;
    Some(IndicatorSpec {
        icon: Some(themed(&charge.icon_name())),
        label: Some(format!("{}%", charge.percentage)),
        severity: severity(charge.warning),
        ..Default::default()
    })
}

pub fn network(state: &NetworkState) -> Option<IndicatorSpec> {
    state.icon_name().map(icon_only)
}

pub fn bluetooth(state: &BluetoothState) -> Option<IndicatorSpec> {
    state.icon_name().map(icon_only)
}

pub fn layout(layouts: &KeyboardLayouts) -> Option<IndicatorSpec> {
    if layouts.layouts.len() < 2 {
        return None;
    }
    let current = layouts
        .current
        .and_then(|index| layouts.layouts.get(usize::from(index)))
        .or_else(|| layouts.layouts.first())?;
    Some(IndicatorSpec {
        label: Some(clean(&current.code, LAYOUT_CAP)),
        ..Default::default()
    })
}

pub fn weather(state: &WeatherProviderState) -> Option<IndicatorSpec> {
    if !state.owner {
        return None;
    }
    let places = &state.status.as_ref()?.places;
    let current = places
        .iter()
        .find(|place| place.place == WatchedPlace::Here && place.current.is_some())
        .or_else(|| places.iter().find(|place| place.current.is_some()))?
        .current
        .as_ref()?;
    Some(IndicatorSpec {
        icon: Some(themed(current.condition.icon_name(current.is_day))),
        label: Some(reading(current.temperature)),
        ..Default::default()
    })
}

pub fn same(a: Option<&IndicatorSpec>, b: Option<&IndicatorSpec>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let icons = match (&a.icon, &b.icon) {
                (None, None) => true,
                (Some(a), Some(b)) => a.equal(Some(b)),
                _ => false,
            };
            icons && a.label == b.label && a.severity == b.severity
        }
        _ => false,
    }
}

pub fn replace(slot: &mut Option<IndicatorSpec>, next: Option<IndicatorSpec>) -> bool {
    if same(slot.as_ref(), next.as_ref()) {
        return false;
    }
    *slot = next;
    true
}

fn severity(level: WarningLevel) -> Option<Severity> {
    match level {
        WarningLevel::Low => Some(Severity::Warning),
        WarningLevel::Critical | WarningLevel::Action => Some(Severity::Error),
        WarningLevel::Unknown | WarningLevel::None | WarningLevel::Discharging => None,
    }
}

fn icon_only(name: &str) -> IndicatorSpec {
    IndicatorSpec {
        icon: Some(themed(name)),
        ..Default::default()
    }
}

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

#[cfg(test)]
mod tests;
