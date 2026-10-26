use glimpse_dbus::upower::WarningLevel;
use glimpse_services::BatteryState;
use glimpse_widgets::{IndicatorSpec, Severity};
use gtk4::gio;
use gtk4::prelude::{Cast, IconExt};

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub battery: Option<IndicatorSpec>,
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

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

#[cfg(test)]
mod tests;
