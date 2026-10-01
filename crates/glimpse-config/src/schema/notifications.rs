use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Notifications {
    /// How many notifications are kept once they have been read. Dismissing one puts it here
    /// rather than destroying it, which is what makes an accidental dismissal recoverable; past
    /// this many, the oldest is dropped for good. Senders choose how often they notify, so this
    /// is the bound that stops a chatty one growing the provider's memory without limit. Clamped
    /// to 1..=1000.
    pub keep: u32,
    /// Regex patterns matched against the application identity, application name, title and body.
    /// Matching notifications are not stored or published.
    pub suppress: Vec<String>,
    pub enabled: bool,
    #[serde(deserialize_with = "optional_monitor")]
    pub monitor: Option<String>,
    pub edge: NotificationEdge,
    #[serde(deserialize_with = "hide_delay")]
    #[schemars(range(min = 1, max = 86400))]
    pub hide_delay: u64,
    #[serde(deserialize_with = "positive_u32")]
    #[schemars(range(min = 1))]
    pub max_items: u32,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            keep: 100,
            suppress: Vec::new(),
            enabled: true,
            monitor: None,
            edge: NotificationEdge::TopRight,
            hide_delay: 4,
            max_items: 6,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationEdge {
    TopLeft,
    TopCenter,
    #[default]
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

fn optional_monitor<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let monitor = Option::<String>::deserialize(deserializer)?;
    if monitor
        .as_deref()
        .is_some_and(|name| name.is_empty() || name.trim() != name)
    {
        return Err(D::Error::custom(
            "monitor must be a non-empty exact connector name without surrounding whitespace",
        ));
    }
    Ok(monitor)
}

fn hide_delay<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u64::deserialize(deserializer)?;
    (1..=86_400)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be between 1 and 86400 seconds"))
}

fn positive_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u32::deserialize(deserializer)?;
    (value > 0)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be greater than zero"))
}

#[cfg(test)]
mod tests;
