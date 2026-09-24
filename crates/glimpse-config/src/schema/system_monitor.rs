use schemars::JsonSchema;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

/// The system-monitor service: what it samples and how often. Whether it samples at all is not a
/// key here — it is derived from panel placement (see `placed_kinds`), because a poll interval
/// only means something once something is actually placed to consume it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct SystemMonitor {
    /// Seconds between samples of every reading this service publishes. Must be between 1 and 60.
    #[serde(deserialize_with = "poll_interval")]
    #[schemars(range(min = 1, max = 60))]
    pub poll_interval: u64,
    /// Filesystem paths sampled for free space, each becoming its own popover tile. A path that
    /// does not exist is skipped. An unmounted-but-present path still resolves — `statvfs` reports
    /// whichever filesystem currently owns the directory, the same way `df` would — accepted, not
    /// a bug. Each entry must be absolute; duplicates (after stripping a trailing `/`) collapse to
    /// their first occurrence.
    #[serde(deserialize_with = "disk_paths")]
    pub disk_paths: Vec<String>,
    /// Whether to probe for an amdgpu GPU at all.
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
mod tests {
    fn load(text: &str) -> Result<crate::Config, crate::ConfigError> {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, text).expect("writes");
        crate::load(Some(&path))
    }

    #[test]
    fn an_absent_table_samples_root_every_two_seconds_with_gpu_on() {
        let system_monitor = load("").expect("an absent table is fine").system_monitor;

        assert_eq!(system_monitor.poll_interval, 2);
        assert_eq!(system_monitor.disk_paths, vec!["/".to_owned()]);
        assert!(system_monitor.gpu);
    }

    #[test]
    fn a_poll_interval_outside_one_to_sixty_is_refused() {
        for value in [0, 61] {
            let rendered = load(&format!("[system-monitor]\npoll-interval = {value}\n"))
                .expect_err("out of range")
                .to_string();
            assert!(
                rendered.contains("poll-interval"),
                "the error must name `poll-interval`, got {rendered}"
            );
        }
    }

    #[test]
    fn a_relative_disk_path_is_refused() {
        let rendered = load("[system-monitor]\ndisk-paths = [\"home\"]\n")
            .expect_err("relative path")
            .to_string();
        assert!(
            rendered.contains("home"),
            "the error must name the offending path, got {rendered}"
        );
    }

    #[test]
    fn duplicate_disk_paths_collapse_to_one_after_normalizing_a_trailing_slash() {
        let system_monitor = load("[system-monitor]\ndisk-paths = [\"/data\", \"/data/\"]\n")
            .expect("both are absolute")
            .system_monitor;

        assert_eq!(system_monitor.disk_paths, vec!["/data".to_owned()]);
    }

    #[test]
    fn an_all_slash_path_normalizes_to_root_rather_than_an_empty_string() {
        let system_monitor = load("[system-monitor]\ndisk-paths = [\"//\", \"///\"]\n")
            .expect("both are absolute")
            .system_monitor;

        assert_eq!(system_monitor.disk_paths, vec!["/".to_owned()]);
    }

    #[test]
    fn a_setting_this_table_does_not_have_is_refused() {
        let rendered = load("[system-monitor]\ninterval = 5\n")
            .expect_err("`interval` is not a key of this table")
            .to_string();
        assert!(
            rendered.contains("interval"),
            "the error must name `interval`, got {rendered}"
        );
    }
}
