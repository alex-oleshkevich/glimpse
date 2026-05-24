use std::collections::HashMap;

use serde::Deserialize;

const DEFAULT_REFRESH_INTERVAL_MS: u64 = 2000;
const DEFAULT_TOP_PROCESSES_COUNT: usize = 5;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// How often samplers run, in milliseconds. The applet uses this both to
    /// schedule the next refresh and to compute delta-based metrics (CPU%,
    /// network rates).
    pub refresh_interval_ms: Option<u64>,
    /// How many rows to surface in the "Top CPU" / "Top RAM" expanders.
    pub top_processes_count: Option<usize>,
    /// Ordered list of indicators. Order maps to panel order.
    pub indicators: Vec<IndicatorConfig>,
    /// Threshold map keyed by metric name (e.g. `"cpu_util"`, `"cpu_temp"`,
    /// `"mem_util"`) or by a disambiguated variant (`"disk:/"`,
    /// `"net:wlan0"`, `"temp:coretemp/Package id 0"`). The disambiguated key
    /// wins when both are present, so a global `cpu_util` threshold can be
    /// overridden per-instance for a specific disk or interface.
    pub thresholds: HashMap<String, ThresholdConfig>,
}

impl Config {
    pub fn refresh_interval_ms(&self) -> u64 {
        self.refresh_interval_ms
            .unwrap_or(DEFAULT_REFRESH_INTERVAL_MS)
    }

    pub fn top_processes_count(&self) -> usize {
        self.top_processes_count
            .unwrap_or(DEFAULT_TOP_PROCESSES_COUNT)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct IndicatorConfig {
    /// Discriminated by the `kind` field. Multi-instance kinds (`disk`,
    /// `net`, `temp`) require their disambiguator key (`mountpoint`,
    /// `interface`, `sensor`) at the same level, e.g.:
    ///
    /// ```toml
    /// [[applets.sysmonitor.indicators]]
    /// kind = "disk"
    /// mountpoint = "/home"
    /// label = "~ {disk_util_pct:.0}%"
    /// ```
    #[serde(flatten)]
    pub kind: IndicatorKind,
    /// Optional explicit icon name. Falls back to a per-kind default when
    /// absent so a sensible glyph still appears on the panel.
    pub icon: Option<String>,
    /// Format string for the panel label. Tokens are resolved against the
    /// current sample; e.g. `"{cpu_util_pct:.0}% {cpu_freq_ghz:.1}G"`.
    pub label: Option<String>,
    /// Format string for the GTK tooltip on hover. Same token surface as
    /// `label`.
    pub tooltip: Option<String>,
    /// Tiny boolean DSL evaluated each tick; `true` hides the indicator.
    /// Example: `"swap_util_pct < 1"` keeps swap hidden when unused.
    pub hide_when: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IndicatorKind {
    Cpu,
    Mem,
    Swap,
    Disk { mountpoint: String },
    Net { interface: String },
    Temp { sensor: String },
    Amdgpu,
    Nvidia,
}

impl IndicatorKind {
    /// Stable identifier used as `StatusItem::id` so the shell can update an
    /// indicator's widget in place across ticks rather than rebuild it.
    /// Multi-instance kinds disambiguate with `kind:<disambiguator>` so
    /// e.g. `/` and `/home` are distinct.
    pub fn id(&self) -> String {
        match self {
            Self::Cpu => "cpu".into(),
            Self::Mem => "mem".into(),
            Self::Swap => "swap".into(),
            Self::Disk { mountpoint } => format!("disk:{mountpoint}"),
            Self::Net { interface } => format!("net:{interface}"),
            Self::Temp { sensor } => format!("temp:{sensor}"),
            Self::Amdgpu => "amdgpu".into(),
            Self::Nvidia => "nvidia".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThresholdConfig {
    /// Lower trip point (inclusive). When the metric is >= warn but < crit
    /// the indicator gets the `threshold-warn` CSS class.
    pub warn: f64,
    /// Upper trip point (inclusive). Above this the indicator gets
    /// `threshold-crit`.
    pub crit: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shell forwards `[applets.sysmonitor.*]` as the applet's options
    /// blob. This is the canonical shape, so any drift in field naming gets
    /// caught here.
    #[test]
    fn full_config_parses_round_trip() {
        let raw = r#"
refresh_interval_ms = 2000
top_processes_count = 5

[[indicators]]
kind = "cpu"
icon = "cpu-symbolic"
label = "{cpu_util_pct:.0}% {cpu_freq_ghz:.1}G"
tooltip = "CPU {cpu_util_pct:.0}% • {cpu_temp:.0}°C"

[[indicators]]
kind = "disk"
mountpoint = "/"
label = "/ {disk_util_pct:.0}%"

[[indicators]]
kind = "disk"
mountpoint = "/home"
label = "~ {disk_util_pct:.0}%"

[[indicators]]
kind = "net"
interface = "wlan0"
label = "↑{net_up_kib_s:.0}K ↓{net_down_kib_s:.0}K"

[[indicators]]
kind = "swap"
hide_when = "swap_util_pct < 1"

[thresholds.cpu_util]
warn = 0.75
crit = 0.9

[thresholds."disk:/"]
warn = 0.85
crit = 0.95
"#;
        let config: Config = toml::from_str(raw).expect("config parses");
        assert_eq!(config.refresh_interval_ms(), 2000);
        assert_eq!(config.top_processes_count(), 5);
        assert_eq!(config.indicators.len(), 5);
        assert_eq!(config.indicators[0].kind, IndicatorKind::Cpu);
        assert_eq!(
            config.indicators[1].kind,
            IndicatorKind::Disk {
                mountpoint: "/".into()
            }
        );
        assert_eq!(config.indicators[2].kind.id(), "disk:/home");
        assert_eq!(config.indicators[3].kind.id(), "net:wlan0");
        assert_eq!(
            config.indicators[4].hide_when.as_deref(),
            Some("swap_util_pct < 1")
        );
        assert_eq!(
            config.thresholds.get("cpu_util"),
            Some(&ThresholdConfig {
                warn: 0.75,
                crit: 0.9
            })
        );
        assert!(config.thresholds.contains_key("disk:/"));
    }

    /// Disambiguators for multi-instance kinds are required by serde — a
    /// `disk` indicator without `mountpoint` is a misconfiguration and
    /// should fail to parse rather than silently fall back to some default.
    #[test]
    fn disk_indicator_requires_mountpoint() {
        let raw = r#"
[[indicators]]
kind = "disk"
"#;
        let err = toml::from_str::<Config>(raw).expect_err("disk without mountpoint must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("mountpoint"),
            "expected `mountpoint` to be cited, got: {msg}"
        );
    }

    /// Defaults: an empty options blob (e.g. a shell entry with no
    /// `[applets.sysmonitor]` table beyond `extends`/`command`) yields a
    /// fully-default Config so the applet can still start up.
    #[test]
    fn empty_config_yields_defaults() {
        let config: Config = toml::from_str("").expect("empty config parses");
        assert_eq!(config.refresh_interval_ms(), DEFAULT_REFRESH_INTERVAL_MS);
        assert_eq!(config.top_processes_count(), DEFAULT_TOP_PROCESSES_COUNT);
        assert!(config.indicators.is_empty());
        assert!(config.thresholds.is_empty());
    }

    /// `deny_unknown_fields` catches typos at the top level so a user typing
    /// `refresh_interval` instead of `refresh_interval_ms` finds out
    /// immediately rather than wondering why their setting is ignored.
    #[test]
    fn unknown_top_level_field_rejected() {
        let raw = r#"
refresh_interval = 2000
"#;
        let err = toml::from_str::<Config>(raw).expect_err("unknown field must fail");
        assert!(err.to_string().contains("unknown field"));
    }
}
