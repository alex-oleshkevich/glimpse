use chrono::NaiveTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MIN_TEMPERATURE: u32 = 1000;
pub const MAX_TEMPERATURE: u32 = 6500;

const CLOCK: &str = "%H:%M";
const CLOCK_PATTERN: &str = r"^([01]?[0-9]|2[0-3]):[0-5]?[0-9]$";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct NightLight {
    /// When your screen uses warmer colors. Automatic mode needs your location to work out sunset
    /// and sunrise.
    pub schedule: Schedule,
    /// Your screen's nighttime color temperature, from 1000 to 6500 kelvin. Lower values look
    /// warmer; 6500 keeps the normal color.
    #[schemars(range(min = MIN_TEMPERATURE, max = MAX_TEMPERATURE))]
    pub temperature: u32,
    /// When warmer colors start with `schedule = "schedule"`, in local `HH:MM` time. Set both times
    /// to different values; the schedule can cross midnight.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("pattern" = CLOCK_PATTERN))]
    pub start_time: Option<String>,
    /// When warmer colors end with `schedule = "schedule"`, in local `HH:MM` time. Set both times
    /// to different values; the schedule can cross midnight.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(extend("pattern" = CLOCK_PATTERN))]
    pub end_time: Option<String>,
    /// How many minutes the change between day and night color takes.
    pub transition_minutes: u32,
    #[serde(skip)]
    pub(crate) temperature_problem: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Document {
    schedule: Option<Schedule>,
    temperature: Option<Kelvin>,
    #[serde(default, deserialize_with = "clock")]
    start_time: Option<String>,
    #[serde(default, deserialize_with = "clock")]
    end_time: Option<String>,
    transition_minutes: Option<u32>,
}

#[derive(Deserialize)]
#[serde(untagged, expecting = "a number of kelvin")]
enum Kelvin {
    Whole(i64),
    Fraction(f64),
}

impl Kelvin {
    fn clamped(self) -> (u32, Option<String>) {
        let (written, whole, exact) = match self {
            Self::Whole(value) => (value.to_string(), value, true),
            Self::Fraction(value) => (format!("{value:?}"), value.round() as i64, false),
        };
        let used = whole.clamp(MIN_TEMPERATURE.into(), MAX_TEMPERATURE.into());
        let problem = (!exact || whole != used).then(|| {
            format!(
                "night-light.temperature: {written} is not a whole number of kelvin in \
                 {MIN_TEMPERATURE}..={MAX_TEMPERATURE}; using {used}"
            )
        });
        (used as u32, problem)
    }
}

impl<'de> Deserialize<'de> for NightLight {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let document = Document::deserialize(deserializer)?;
        let defaults = Self::default();
        let (temperature, temperature_problem) = match document.temperature {
            Some(kelvin) => kelvin.clamped(),
            None => (defaults.temperature, None),
        };
        Ok(Self {
            schedule: document.schedule.unwrap_or(defaults.schedule),
            temperature,
            start_time: document.start_time,
            end_time: document.end_time,
            transition_minutes: document
                .transition_minutes
                .unwrap_or(defaults.transition_minutes),
            temperature_problem,
        })
    }
}

fn clock<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<String>::deserialize(deserializer)?;
    if let Some(text) = &raw {
        parse_clock(text).map_err(serde::de::Error::custom)?;
    }
    Ok(raw)
}

pub fn parse_clock(raw: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(raw, CLOCK)
        .map_err(|_| format!("expected a time written as HH:MM, got {raw:?}"))
}

impl Default for NightLight {
    fn default() -> Self {
        Self {
            schedule: Schedule::Automatic,
            temperature: 4200,
            start_time: None,
            end_time: None,
            transition_minutes: 15,
            temperature_problem: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Schedule {
    Off,
    Automatic,
    Schedule,
}

impl Schedule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Automatic => "automatic",
            Self::Schedule => "schedule",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [Self::Off, Self::Automatic, Self::Schedule]
            .into_iter()
            .find(|mode| mode.as_str() == raw)
    }
}

#[cfg(test)]
mod tests;
