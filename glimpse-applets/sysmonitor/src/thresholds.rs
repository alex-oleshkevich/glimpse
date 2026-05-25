//! Maps a metric value to a CSS class name for panel tinting. The class
//! ("threshold-warn" or "threshold-crit") is then attached to the
//! `StatusItem` via the css_classes field added in the protocol bump and
//! flows through `PanelIndicator::set_extra_classes` on the shell side.

use crate::config::Config;

/// Resolves the most appropriate severity class for `value` against
/// thresholds defined in config. Checks the disambiguated key first
/// (e.g. `"disk:/home"`, `"net:wlan0"`, `"temp:coretemp"`) so a per-
/// instance override beats the unprefixed global key
/// (`"disk_util"`, `"net_..."`, `"temp"`). Returns `None` when no
/// threshold for either key is configured or when the value falls
/// below the warn level.
pub fn resolve(
    config: &Config,
    instance_key: Option<&str>,
    metric_key: &str,
    value: f64,
) -> Option<&'static str> {
    let threshold = instance_key
        .and_then(|k| config.thresholds.get(k))
        .or_else(|| config.thresholds.get(metric_key))?;
    if value >= threshold.crit {
        Some("threshold-crit")
    } else if value >= threshold.warn {
        Some("threshold-warn")
    } else {
        None
    }
}

/// Picks the most-severe class out of several candidate evaluations.
/// Indicators that watch multiple metrics (CPU watches both util and
/// temp; GPU watches util, temp, and mem) call this so the panel shows
/// the worst of them.
pub fn most_severe<'a>(candidates: impl IntoIterator<Item = Option<&'a str>>) -> Option<&'a str> {
    let mut best: Option<&str> = None;
    for class in candidates.into_iter().flatten() {
        match (best, class) {
            (None, c) => best = Some(c),
            (Some("threshold-warn"), "threshold-crit") => best = Some(class),
            _ => {}
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThresholdConfig;
    use std::collections::HashMap;

    fn config_with(map: &[(&str, f64, f64)]) -> Config {
        let mut thresholds = HashMap::new();
        for (k, warn, crit) in map {
            thresholds.insert((*k).into(), ThresholdConfig { warn: *warn, crit: *crit });
        }
        Config {
            thresholds,
            ..Config::default()
        }
    }

    /// 75% util with warn=0.75/crit=0.9 must cross into warn (inclusive
    /// boundary). Off-by-one here would have users seeing yellow at 76%
    /// instead of 75% — the user-visible quality of the threshold UX.
    #[test]
    fn warn_boundary_is_inclusive() {
        let cfg = config_with(&[("cpu_util", 0.75, 0.9)]);
        assert_eq!(resolve(&cfg, None, "cpu_util", 0.75), Some("threshold-warn"));
        assert_eq!(resolve(&cfg, None, "cpu_util", 0.74), None);
    }

    /// Above crit must be crit. Equal to crit must be crit (inclusive).
    #[test]
    fn crit_boundary_is_inclusive() {
        let cfg = config_with(&[("cpu_util", 0.75, 0.9)]);
        assert_eq!(resolve(&cfg, None, "cpu_util", 0.9), Some("threshold-crit"));
        assert_eq!(resolve(&cfg, None, "cpu_util", 0.95), Some("threshold-crit"));
    }

    /// A per-instance threshold (e.g. `"disk:/"`) overrides the global
    /// metric threshold (`"disk_util"`). This lets users keep a relaxed
    /// global default and tighten just the home partition.
    #[test]
    fn instance_key_beats_metric_key() {
        let cfg = config_with(&[
            ("disk_util", 0.85, 0.95),
            ("disk:/home", 0.50, 0.60),
        ]);
        // 0.55 fires warn under the per-instance key but is below the
        // global key's warn (0.85), so without the override it'd be None.
        assert_eq!(
            resolve(&cfg, Some("disk:/home"), "disk_util", 0.55),
            Some("threshold-warn"),
        );
    }

    /// When neither key is configured, no class is applied — the indicator
    /// renders with just the base styling.
    #[test]
    fn missing_threshold_returns_none() {
        let cfg = config_with(&[]);
        assert_eq!(resolve(&cfg, Some("disk:/"), "disk_util", 0.99), None);
    }

    /// `most_severe` picks crit over warn regardless of evaluation order.
    /// This is the CPU case: util might be warn while temp is crit;
    /// indicator should be tinted crit.
    #[test]
    fn most_severe_picks_crit_over_warn_in_either_order() {
        assert_eq!(
            most_severe([Some("threshold-warn"), Some("threshold-crit"), None]),
            Some("threshold-crit"),
        );
        assert_eq!(
            most_severe([Some("threshold-crit"), Some("threshold-warn"), None]),
            Some("threshold-crit"),
        );
        assert_eq!(most_severe([None, Some("threshold-warn"), None]), Some("threshold-warn"));
        assert_eq!(most_severe([None, None]), None);
    }
}
