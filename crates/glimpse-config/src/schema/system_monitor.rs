use schemars::JsonSchema;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// The system-monitor service: what it samples and how often. Whether it samples at all is not a
/// key here — it is derived from panel placement (see `placed_kinds`), because a poll interval
/// only means something once something is actually placed to consume it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct SystemMonitor {
    /// How often the readings update, in seconds, from 1 to 60.
    #[serde(deserialize_with = "poll_interval")]
    #[schemars(range(min = 1, max = 60))]
    pub poll_interval: u64,
    /// Full paths to folders on the disks you want to check for free space.
    #[serde(deserialize_with = "disk_paths")]
    pub disk_paths: Vec<String>,
    /// Show readings from your AMD GPU.
    pub gpu: bool,
}

impl Default for SystemMonitor {
    fn default() -> Self {
        Self {
            poll_interval: 2,
            disk_paths: vec!["/".to_owned()],
            gpu: true,
        }
    }
}

fn poll_interval<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u64::deserialize(deserializer)?;
    (1..=60)
        .contains(&value)
        .then_some(value)
        .ok_or_else(|| D::Error::custom("must be between 1 and 60 seconds"))
}

fn disk_paths<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Vec::<String>::deserialize(deserializer)?;
    let mut seen = std::collections::HashSet::new();
    let mut paths = Vec::new();
    for path in raw {
        if !path.starts_with('/') {
            return Err(D::Error::custom(format!(
                "`{path}` is not an absolute path"
            )));
        }
        let trimmed = path.trim_end_matches('/');
        let normalized = if trimmed.is_empty() {
            "/".to_owned()
        } else {
            trimmed.to_owned()
        };
        if seen.insert(normalized.clone()) {
            paths.push(normalized);
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests;
