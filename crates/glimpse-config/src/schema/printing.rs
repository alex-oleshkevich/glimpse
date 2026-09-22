use schemars::JsonSchema;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// The printing service: how often it polls CUPS for job state, and which server it polls.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Printing {
    /// How often the service polls CUPS while a job is active, in seconds. Must be at least 1;
    /// `0` would leave the poll timer with no interval to tick on.
    #[serde(deserialize_with = "poll_seconds")]
    #[schemars(range(min = 1))]
    pub poll_active: u64,
    /// How often the service polls CUPS while no job is active, in seconds. Must be at least 1,
    /// for the same reason as `poll-active`.
    #[serde(deserialize_with = "poll_seconds")]
    #[schemars(range(min = 1))]
    pub poll_idle: u64,
    /// The CUPS server to poll, such as `http://localhost:631/`. Unset uses CUPS's own default
    /// server.
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
mod tests {
    #[test]
    fn a_zero_poll_interval_is_refused() {
        for key in ["poll-active", "poll-idle"] {
            let document = format!("[printing]\n{key} = 0\n");
            toml::from_str::<crate::Config>(&document)
                .expect_err("a zero interval has no timer to tick on");
        }
    }
}
