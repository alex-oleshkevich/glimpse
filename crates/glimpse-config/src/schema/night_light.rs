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
