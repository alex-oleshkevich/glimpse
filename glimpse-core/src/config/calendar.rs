use serde::{Deserialize, Serialize};

pub const DEFAULT_CALENDAR_POLL_INTERVAL: u64 = 600;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct CalendarConfig {
    pub poll_interval: u64,
    #[serde(deserialize_with = "deserialize_sources")]
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

/// Deserializes `[[calendar.sources]]` leniently: a malformed entry (missing
/// `id`/`type`/`uri`, bad `type` value, ...) is skipped with a warning
/// instead of failing the whole `Config` document — one typo'd calendar
/// source must not reset every other setting to defaults.
fn deserialize_sources<'de, D>(deserializer: D) -> Result<Vec<CalendarSourceConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Vec::<toml::Value>::deserialize(deserializer)?;
    let mut sources = Vec::with_capacity(raw.len());
    for (index, value) in raw.into_iter().enumerate() {
        match CalendarSourceConfig::deserialize(value) {
            Ok(source) => sources.push(source),
            Err(error) => {
                tracing::warn!(index, %error, "skipping invalid calendar source config entry");
            }
        }
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_calendar_source_is_skipped_with_others_kept() {
        let config: CalendarConfig = toml::from_str(
            r#"
poll_interval = 300

[[sources]]
id = "good"
type = "ical"
uri = "file:///a.ics"

[[sources]]
type = "ical"
uri = "file:///missing-id.ics"

[[sources]]
id = "good-2"
type = "directory"
uri = "file:///dir"
"#,
        )
        .expect("a malformed source entry must not fail the whole document");

        assert_eq!(config.sources.len(), 2);
        assert_eq!(config.sources[0].id, "good");
        assert_eq!(config.sources[1].id, "good-2");
    }

    #[test]
    fn all_valid_calendar_sources_are_kept() {
        let config: CalendarConfig = toml::from_str(
            r#"
[[sources]]
id = "a"
type = "ical"
uri = "file:///a.ics"
"#,
        )
        .unwrap();

        assert_eq!(config.sources.len(), 1);
    }
}
