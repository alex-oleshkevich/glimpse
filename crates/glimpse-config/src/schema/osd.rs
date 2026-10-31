use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use super::NotificationEdge;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Osd {
    /// Whether the on-screen display appears for volume, brightness and keyboard layout changes.
    pub enabled: bool,
    /// The corner or edge of the monitor the display appears at. It appears on the monitor
    /// notifications use.
    pub edge: NotificationEdge,
    /// Milliseconds the display stays after the last change.
    #[serde(deserialize_with = "duration")]
    #[schemars(range(min = 500, max = 10000))]
    pub duration: u64,
}

impl Default for Osd {
    fn default() -> Self {
        Self {
            enabled: true,
            edge: NotificationEdge::BottomCenter,
            duration: 1500,
        }
    }
}

fn duration<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u64::deserialize(deserializer)?;
    (500..=10_000)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be between 500 and 10000 milliseconds"))
}

#[cfg(test)]
mod tests;
