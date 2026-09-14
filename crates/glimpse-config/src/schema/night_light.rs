use chrono::NaiveTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const CLOCK: &str = "%H:%M";
const CLOCK_PATTERN: &str = r"^([01]?[0-9]|2[0-3]):[0-5]?[0-9]$";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct NightLight {
    pub schedule: Schedule,
    pub temperature: u32,
    #[serde(deserialize_with = "clock", skip_serializing_if = "Option::is_none")]
    #[schemars(extend("pattern" = CLOCK_PATTERN))]
    pub start_time: Option<String>,
    #[serde(deserialize_with = "clock", skip_serializing_if = "Option::is_none")]
    #[schemars(extend("pattern" = CLOCK_PATTERN))]
    pub end_time: Option<String>,
    pub transition_minutes: u32,
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
mod tests {
    use super::*;

    #[test]
    fn every_spelling_is_one_the_document_reads_back_as_the_same_mode() {
        for mode in [Schedule::Off, Schedule::Automatic, Schedule::Schedule] {
            let parsed: NightLight = toml::from_str(&format!("schedule = \"{}\"\n", mode.as_str()))
                .expect("the spelling is one serde accepts");
            assert_eq!(parsed.schedule, mode);
            assert_eq!(Schedule::parse(mode.as_str()), Some(mode));
        }
    }

    #[test]
    fn a_time_the_runtime_cannot_read_is_refused_at_load() {
        for spelling in ["10pm", "22:00:00", "24:00", "noon", ""] {
            let error = toml::from_str::<NightLight>(&format!("start-time = \"{spelling}\"\n"))
                .expect_err("a time the schedule could never use");
            assert!(error.to_string().contains("HH:MM"), "{spelling}: {error}");
        }
    }

    #[test]
    fn both_keys_are_checked_not_only_the_first() {
        let error = toml::from_str::<NightLight>("start-time = \"20:00\"\nend-time = \"7pm\"\n")
            .expect_err("end-time is checked too");
        assert!(error.to_string().contains("HH:MM"), "{error}");
    }

    #[test]
    fn the_emitted_schema_constrains_both_times_rather_than_leaving_it_to_the_loader() {
        let document: serde_json::Value = serde_json::from_str(&crate::json_schema_document())
            .expect("the emitted schema is JSON");
        let night_light = &document["$defs"]["NightLight"]["properties"];

        for key in ["start-time", "end-time"] {
            assert_eq!(
                night_light[key]["pattern"], CLOCK_PATTERN,
                "{key} is an unconstrained string in the schema, so an editor accepts a value the \
                 loader then refuses"
            );
        }
    }

    #[test]
    fn a_time_written_as_hh_mm_loads_unchanged() {
        let parsed: NightLight = toml::from_str("start-time = \"20:00\"\nend-time = \"07:00\"\n")
            .expect("both are HH:MM");

        assert_eq!(parsed.start_time.as_deref(), Some("20:00"));
        assert_eq!(parsed.end_time.as_deref(), Some("07:00"));
    }

    #[test]
    fn absent_times_are_absent_rather_than_refused() {
        let parsed: NightLight = toml::from_str("schedule = \"schedule\"\n").expect("no times");

        assert_eq!(parsed.start_time, None);
        assert_eq!(parsed.end_time, None);
    }

    #[test]
    fn the_document_only_alias_is_not_a_mode_a_caller_can_name() {
        assert_eq!(Schedule::parse("manual"), None);
        assert_eq!(Schedule::parse("Off"), None);
        assert_eq!(Schedule::parse(""), None);
    }
}
