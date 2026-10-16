use schemars::JsonSchema;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// The printing service: how often it polls CUPS for job state, and which server it polls.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Printing {
    /// How often print jobs are checked while something is printing, in seconds, with a minimum of 1.
    #[serde(deserialize_with = "poll_seconds")]
    #[schemars(range(min = 1))]
    pub poll_active: u64,
    /// How often print jobs are checked while nothing is printing, in seconds, with a minimum of 1.
    #[serde(deserialize_with = "poll_seconds")]
    #[schemars(range(min = 1))]
    pub poll_idle: u64,
    /// Your print server's address, such as `http://localhost:631/`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
}

impl Default for Printing {
    fn default() -> Self {
        Self {
            poll_active: 2,
            poll_idle: 30,
            server_url: None,
        }
    }
}

fn poll_seconds<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u64::deserialize(deserializer)?;
    (value > 0)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be at least 1 second"))
}

#[cfg(test)]
mod tests;
