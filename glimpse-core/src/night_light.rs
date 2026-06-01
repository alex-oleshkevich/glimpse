use serde::Serialize;

pub const DAYLIGHT_TEMPERATURE_KELVIN: u32 = 6500;

#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum NightLightPhase {
    #[default]
    Disabled,
    Day,
    TransitionToNight,
    Night,
    TransitionToDay,
}
