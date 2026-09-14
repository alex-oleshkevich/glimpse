use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct NightLight {
    pub schedule: Schedule,
    pub temperature: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    pub transition_minutes: u32,
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
    fn the_document_only_alias_is_not_a_mode_a_caller_can_name() {
        assert_eq!(Schedule::parse("manual"), None);
        assert_eq!(Schedule::parse("Off"), None);
        assert_eq!(Schedule::parse(""), None);
    }
}
