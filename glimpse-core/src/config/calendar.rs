use serde::{Deserialize, Serialize};

pub const DEFAULT_CALENDAR_POLL_INTERVAL: u64 = 600;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct CalendarConfig {
    pub poll_interval: u64,
    pub sources: Vec<CalendarSourceConfig>,
}

impl Default for CalendarConfig {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_CALENDAR_POLL_INTERVAL,
            sources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalendarSourceType {
    Ical,
    Directory,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CalendarSourceConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub source_type: CalendarSourceType,
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_interval: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}
