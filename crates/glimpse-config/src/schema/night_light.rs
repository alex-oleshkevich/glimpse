use chrono::NaiveTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MIN_TEMPERATURE: u32 = 1000;
pub const MAX_TEMPERATURE: u32 = 6500;

pub const CLOCK: &str = "%H:%M";
const CLOCK_PATTERN: &str = r"^([01]?[0-9]|2[0-3]):[0-5]?[0-9]$";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct NightLight {
    pub schedule: Schedule,
    #[serde(deserialize_with = "temperature")]
    #[schemars(range(min = MIN_TEMPERATURE, max = MAX_TEMPERATURE))]
    pub temperature: u32,
    #[serde(deserialize_with = "clock", skip_serializing_if = "Option::is_none")]
    #[schemars(extend("pattern" = CLOCK_PATTERN))]
    pub start_time: Option<String>,
    #[serde(deserialize_with = "clock", skip_serializing_if = "Option::is_none")]
    #[schemars(extend("pattern" = CLOCK_PATTERN))]
    pub end_time: Option<String>,
    pub transition_minutes: u32,
}

fn temperature<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = u32::deserialize(deserializer)?;
    (MIN_TEMPERATURE..=MAX_TEMPERATURE)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| {
            serde::de::Error::custom(format!(
                "must be between {MIN_TEMPERATURE} and {MAX_TEMPERATURE} kelvin"
            ))
        })
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Schedule {
    Off,
    Automatic,
    #[serde(alias = "manual")]
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
