// `collapsible_if` / `collapsible_match` flag the per-`IndicatorKind`
// arms in `requested_sensors` where a single `if !already_present { push }`
// body could fold into a match guard. The expanded form is easier to
// scan when adding new indicator kinds, so we keep it.
#![allow(clippy::collapsible_if, clippy::collapsible_match)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use glimpse_sdk::{Applet, AppletResult, InitEvent, StatusItem, TreeNode, mpsc, run};

mod config;
mod format;
mod popover;
mod samplers;
mod thresholds;

use config::{Config, IndicatorConfig, IndicatorKind};
use format::FormatValue;
use samplers::{
    RequestedSensors, Sample, Samplers, discover_interfaces, discover_mounts, discover_sensors,
};

/// Held in the SDK's State channel. Clone-cheap (Sample is `Copy`-able
/// numeric fields, Config is small).
#[derive(Debug, Clone, Default)]
struct State {
    config: Config,
    sample: Option<Sample>,
}

/// Inbound messages to the applet's update loop.
#[derive(Debug, Clone, PartialEq)]
enum Msg {
    /// Latest sampler reading. Boxed because `Sample` carries three
    /// `HashMap`s (disks / nets / temps) and outweighs the other
    /// variants by an order of magnitude — clippy's
    /// `large_enum_variant` catches the imbalance, and boxing keeps
    /// the channel queue items uniformly small.
    Tick(Box<Sample>),
}

struct SysmonitorApplet;

#[async_trait]
impl Applet for SysmonitorApplet {
    type State = State;
    type Msg = Msg;

    async fn on_init(&mut self, state: &mut State, event: InitEvent) -> AppletResult<()> {
        match serde_json::from_value::<Config>(event.options.clone()) {
            Ok(config) => state.config = config,
            Err(error) => {
                eprintln!("sysmonitor: invalid applet config: {error}");
            }
        }
        // Sane defaults: when the user hasn't configured any indicators,
        // emit a sensible default set (CPU util, mem util, CPU temp) so
        // the applet does something useful out of the box. User-defined
        // thresholds win over defaults; we only fill in keys they
        // haven't set, so a custom `cpu_util` threshold isn't clobbered.
        if state.config.indicators.is_empty() {
            state.config.indicators = config::default_indicators();
        }
        for (key, threshold) in config::default_thresholds() {
            state.config.thresholds.entry(key).or_insert(threshold);
        }
        Ok(())
    }

    async fn on_start(
        &mut self,
        state: &mut State,
        tx: mpsc::Sender<Msg>,
    ) -> AppletResult<()> {
        let interval_ms = state.config.refresh_interval_ms();
        let interval = Duration::from_millis(interval_ms);
        let requested = requested_sensors(&state.config);
        tokio::spawn(async move {
            let mut samplers = if requested.nvidia {
                Samplers::with_nvidia(interval_ms).await
            } else {
                Samplers::new()
            };
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // First tick fires immediately so the panel doesn't sit empty.
            // CPU util will read 0.0 on the first sample (no previous
            // /proc/stat snapshot to delta against); the second tick is
            // accurate.
            loop {
                ticker.tick().await;
                let sample = samplers.tick(&requested);
                if tx.send(Msg::Tick(Box::new(sample))).await.is_err() {
                    // Channel closed — applet is shutting down.
                    break;
                }
            }
        });
        Ok(())
    }

    async fn update(&mut self, state: &mut State, msg: Msg) -> AppletResult<()> {
        match msg {
            Msg::Tick(sample) => state.sample = Some(*sample),
        }
        Ok(())
    }

    async fn status(&self, state: &State) -> AppletResult<Vec<StatusItem>> {
        let Some(sample) = state.sample.as_ref() else {
            return Ok(Vec::new());
        };
        let items = state
            .config
            .indicators
            .iter()
            .filter_map(|indicator| render_indicator(indicator, sample, &state.config))
            .collect();
        Ok(items)
    }

    async fn popover(&self, state: &State) -> AppletResult<Option<TreeNode<Msg>>> {
        let Some(sample) = state.sample.as_ref() else {
            return Ok(None);
        };
        Ok(Some(popover::build(sample)))
    }
}

/// Render one indicator into a `StatusItem`. Returns `None` to omit the
/// indicator entirely — either because the configured kind has no
/// implementation yet (disk/net/temp/gpu land in later tasks) or because
/// the indicator's `hide_when` condition evaluated to true.
fn render_indicator(
    indicator: &IndicatorConfig,
    sample: &Sample,
    config: &Config,
) -> Option<StatusItem> {
    let mut values = HashMap::new();
    populate_shared_tokens(&mut values, sample);
    let (metric_class, supported) = match &indicator.kind {
        IndicatorKind::Cpu => {
            populate_cpu_tokens(&mut values, sample);
            (cpu_threshold(config, sample), true)
        }
        IndicatorKind::Mem => {
            populate_mem_tokens(&mut values, "mem", &sample.mem);
            (
                thresholds::resolve(config, None, "mem_util", sample.mem.util),
                true,
            )
        }
        IndicatorKind::Swap => {
            populate_mem_tokens(&mut values, "swap", &sample.swap);
            (
                thresholds::resolve(config, None, "swap_util", sample.swap.util),
                true,
            )
        }
        IndicatorKind::Disk { mountpoint } => {
            let disk = sample.disks.get(mountpoint.as_str())?;
            populate_disk_tokens(&mut values, mountpoint, disk);
            let instance_key = format!("disk:{mountpoint}");
            (
                thresholds::resolve(config, Some(&instance_key), "disk_util", disk.util),
                true,
            )
        }
        IndicatorKind::Net { interface } => {
            let net = sample.nets.get(interface.as_str())?;
            populate_net_tokens(&mut values, interface, net);
            // No threshold metric for net by default — users typically don't
            // colour-code network throughput. They can still configure
            // `[thresholds."net:wlan0"]` against the per-second token if
            // desired, but we leave the default off.
            (None, true)
        }
        IndicatorKind::Temp { sensor } => {
            let temp = sample.temps.get(sensor.as_str())?;
            populate_temp_tokens(&mut values, sensor, temp);
            let instance_key = format!("temp:{sensor}");
            (
                thresholds::resolve(config, Some(&instance_key), "temp", temp.temp_c),
                true,
            )
        }
        IndicatorKind::Amdgpu => {
            let gpu = sample.amdgpu.as_ref()?;
            populate_gpu_tokens(&mut values, gpu);
            (gpu_threshold(config, "amdgpu", gpu), true)
        }
        IndicatorKind::Nvidia => {
            let gpu = sample.nvidia.as_ref()?;
            populate_gpu_tokens(&mut values, gpu);
            (gpu_threshold(config, "nvidia", gpu), true)
        }
    };
    if !supported {
        return None;
    }

    // `indicator.id` lets multiple indicators of the same kind coexist
    // on the panel (e.g. default config has two `kind = "cpu"` rows,
    // distinguished by ids `cpu-util` and `cpu-temp`). When unset we
    // fall back to the kind-derived id, which is unique for everything
    // except the multi-cpu case.
    let id = indicator
        .id
        .clone()
        .unwrap_or_else(|| indicator.kind.id());
    let icon = indicator
        .icon
        .as_deref()
        .unwrap_or_else(|| default_icon(&indicator.kind))
        .to_owned();
    let label = indicator
        .label
        .as_deref()
        .map(|t| format::render(t, &values))
        .unwrap_or_default();
    let tooltip = indicator
        .tooltip
        .as_deref()
        .map(|t| format::render(t, &values));

    let mut item = StatusItem::new(id).icon(icon).label(label);
    if let Some(tip) = tooltip {
        item = item.tooltip(tip);
    }
    if let Some(class) = metric_class {
        item = item.css_class(class);
    }
    Some(item)
}

fn cpu_threshold(config: &Config, sample: &Sample) -> Option<&'static str> {
    let util_class = thresholds::resolve(config, None, "cpu_util", sample.cpu.util);
    let temp_class = sample
        .cpu
        .temp_c
        .and_then(|t| thresholds::resolve(config, None, "cpu_temp", t));
    thresholds::most_severe([util_class, temp_class])
}

fn populate_shared_tokens(values: &mut HashMap<&str, FormatValue>, _sample: &Sample) {
    // Hostname/uptime land here in the next task; gated to keep the first
    // sampler tick fast and avoid system calls per render.
    let _ = values; // suppress unused-warning until shared tokens land
}

fn populate_cpu_tokens(values: &mut HashMap<&str, FormatValue>, sample: &Sample) {
    values.insert("cpu_util", sample.cpu.util.into());
    values.insert("cpu_util_pct", (sample.cpu.util * 100.0).into());
    values.insert("cpu_freq_mhz", sample.cpu.freq_mhz.into());
    values.insert("cpu_freq_ghz", (sample.cpu.freq_mhz / 1000.0).into());
    if let Some(temp) = sample.cpu.temp_c {
        values.insert("cpu_temp", temp.into());
    }
    values.insert("cpu_cores", sample.cpu.cores.into());
    values.insert("load_1", sample.cpu.load_1.into());
    values.insert("load_5", sample.cpu.load_5.into());
    values.insert("load_15", sample.cpu.load_15.into());
}

fn populate_mem_tokens(
    values: &mut HashMap<&str, FormatValue>,
    prefix: &str,
    mem: &samplers::MemSample,
) {
    // Each call gets its own static prefix (`"mem"` / `"swap"`) so the same
    // helper feeds both kinds without an extra abstraction layer.
    let key = |suffix: &'static str| -> &'static str {
        // Cheap: we're returning into a HashMap that lives only for this
        // tick, so the leak from Box::leak-style would be unnecessary —
        // use a small match table instead.
        match (prefix, suffix) {
            ("mem", "used_bytes") => "mem_used_bytes",
            ("mem", "free_bytes") => "mem_free_bytes",
            ("mem", "avail_bytes") => "mem_avail_bytes",
            ("mem", "total_bytes") => "mem_total_bytes",
            ("mem", "used_mib") => "mem_used_mib",
            ("mem", "used_gib") => "mem_used_gib",
            ("mem", "free_gib") => "mem_free_gib",
            ("mem", "avail_gib") => "mem_avail_gib",
            ("mem", "total_gib") => "mem_total_gib",
            ("mem", "util") => "mem_util",
            ("mem", "util_pct") => "mem_util_pct",
            ("swap", "used_bytes") => "swap_used_bytes",
            ("swap", "free_bytes") => "swap_free_bytes",
            ("swap", "avail_bytes") => "swap_avail_bytes",
            ("swap", "total_bytes") => "swap_total_bytes",
            ("swap", "used_mib") => "swap_used_mib",
            ("swap", "used_gib") => "swap_used_gib",
            ("swap", "free_gib") => "swap_free_gib",
            ("swap", "total_gib") => "swap_total_gib",
            ("swap", "util") => "swap_util",
            ("swap", "util_pct") => "swap_util_pct",
            _ => unreachable!("unmapped (prefix, suffix) in populate_mem_tokens"),
        }
    };

    let to_mib = |b: u64| (b as f64) / (1024.0 * 1024.0);
    let to_gib = |b: u64| (b as f64) / (1024.0 * 1024.0 * 1024.0);

    values.insert(key("used_bytes"), mem.used_bytes.into());
    values.insert(key("free_bytes"), mem.free_bytes.into());
    if prefix == "mem" {
        values.insert(key("avail_bytes"), mem.avail_bytes.into());
        values.insert(key("avail_gib"), to_gib(mem.avail_bytes).into());
    } else {
        values.insert(key("avail_bytes"), mem.avail_bytes.into());
    }
    values.insert(key("total_bytes"), mem.total_bytes.into());
    values.insert(key("used_mib"), to_mib(mem.used_bytes).into());
    values.insert(key("used_gib"), to_gib(mem.used_bytes).into());
    values.insert(key("free_gib"), to_gib(mem.free_bytes).into());
    values.insert(key("total_gib"), to_gib(mem.total_bytes).into());
    values.insert(key("util"), mem.util.into());
    values.insert(key("util_pct"), (mem.util * 100.0).into());
}

fn populate_disk_tokens(
    values: &mut HashMap<&str, FormatValue>,
    mountpoint: &str,
    disk: &samplers::DiskSample,
) {
    let to_gib = |b: u64| (b as f64) / (1024.0 * 1024.0 * 1024.0);
    values.insert("disk_used_bytes", disk.used_bytes.into());
    values.insert("disk_free_bytes", disk.free_bytes.into());
    values.insert("disk_total_bytes", disk.total_bytes.into());
    values.insert("disk_used_gib", to_gib(disk.used_bytes).into());
    values.insert("disk_free_gib", to_gib(disk.free_bytes).into());
    values.insert("disk_total_gib", to_gib(disk.total_bytes).into());
    values.insert("disk_util", disk.util.into());
    values.insert("disk_util_pct", (disk.util * 100.0).into());
    values.insert("mountpoint", mountpoint.to_owned().into());
}

fn populate_net_tokens(
    values: &mut HashMap<&str, FormatValue>,
    interface: &str,
    net: &samplers::NetSample,
) {
    let to_kib = |b: f64| b / 1024.0;
    let to_mib = |b: f64| b / (1024.0 * 1024.0);
    values.insert("net_up_bytes_s", net.tx_bytes_per_sec.into());
    values.insert("net_down_bytes_s", net.rx_bytes_per_sec.into());
    values.insert("net_up_kib_s", to_kib(net.tx_bytes_per_sec).into());
    values.insert("net_down_kib_s", to_kib(net.rx_bytes_per_sec).into());
    values.insert("net_up_mib_s", to_mib(net.tx_bytes_per_sec).into());
    values.insert("net_down_mib_s", to_mib(net.rx_bytes_per_sec).into());
    values.insert("net_up_total_bytes", net.tx_total_bytes.into());
    values.insert("net_down_total_bytes", net.rx_total_bytes.into());
    values.insert("interface", interface.to_owned().into());
}

fn populate_temp_tokens(
    values: &mut HashMap<&str, FormatValue>,
    sensor_spec: &str,
    temp: &samplers::TempSample,
) {
    values.insert("temp", temp.temp_c.into());
    values.insert("sensor_name", sensor_spec.to_owned().into());
    if let Some(label) = temp.sensor_label.as_deref() {
        values.insert("sensor_label", label.to_owned().into());
    }
}

/// Builds the `RequestedSensors` list.
///
/// Two layers:
/// 1. User-named items from `config.indicators` go first so panel
///    ordering matches config order.
/// 2. Auto-discovered hardware (real-fs mountpoints, up interfaces,
///    every hwmon temp input, both GPU vendors) is appended de-duped
///    so the popover surfaces sections the user didn't pin to a panel
///    pill. The samplers themselves gate on availability — `amdgpu` /
///    `nvidia` short-circuit when the hardware isn't present.
fn requested_sensors(config: &Config) -> RequestedSensors {
    let mut req = RequestedSensors::default();
    for indicator in &config.indicators {
        match &indicator.kind {
            IndicatorKind::Disk { mountpoint } => {
                let path = PathBuf::from(mountpoint);
                if !req.mountpoints.iter().any(|p| p == &path) {
                    req.mountpoints.push(path);
                }
            }
            IndicatorKind::Net { interface } => {
                if !req.interfaces.iter().any(|i| i == interface) {
                    req.interfaces.push(interface.clone());
                }
            }
            IndicatorKind::Temp { sensor } => {
                if !req.temp_sensors.iter().any(|s| s == sensor) {
                    req.temp_sensors.push(sensor.clone());
                }
            }
            IndicatorKind::Amdgpu => req.amdgpu = true,
            IndicatorKind::Nvidia => req.nvidia = true,
            _ => {}
        }
    }
    for path in discover_mounts() {
        if !req.mountpoints.iter().any(|p| p == &path) {
            req.mountpoints.push(path);
        }
    }
    for iface in discover_interfaces() {
        if !req.interfaces.iter().any(|i| i == &iface) {
            req.interfaces.push(iface);
        }
    }
    for sensor in discover_sensors() {
        if !req.temp_sensors.iter().any(|s| s == &sensor) {
            req.temp_sensors.push(sensor);
        }
    }
    // GPU samplers internally gate on hardware presence so requesting
    // both is safe on machines that have neither.
    req.amdgpu = true;
    req.nvidia = true;
    // Top-processes UI was removed from the popover; setting this to
    // zero skips the per-tick `/proc` walk entirely.
    req.top_processes_count = 0;
    req
}

fn populate_gpu_tokens(values: &mut HashMap<&str, FormatValue>, gpu: &samplers::GpuSample) {
    if let Some(util) = gpu.util {
        values.insert("gpu_util", util.into());
        values.insert("gpu_util_pct", (util * 100.0).into());
    }
    if let Some(temp) = gpu.temp_c {
        values.insert("gpu_temp", temp.into());
    }
    if let Some(used) = gpu.mem_used_bytes {
        values.insert("gpu_mem_used_mib", ((used as f64) / (1024.0 * 1024.0)).into());
    }
    if let Some(total) = gpu.mem_total_bytes {
        values.insert("gpu_mem_total_mib", ((total as f64) / (1024.0 * 1024.0)).into());
    }
    if let Some(util) = gpu.mem_util {
        values.insert("gpu_mem_util", util.into());
        values.insert("gpu_mem_util_pct", (util * 100.0).into());
    }
    if let Some(freq) = gpu.freq_mhz {
        values.insert("gpu_freq_mhz", freq.into());
    }
    if let Some(power) = gpu.power_w {
        values.insert("gpu_power_w", power.into());
    }
    if let Some(name) = gpu.name.as_deref() {
        values.insert("gpu_name", name.to_owned().into());
    }
}

fn gpu_threshold(
    config: &Config,
    instance: &str,
    gpu: &samplers::GpuSample,
) -> Option<&'static str> {
    // GPU watches util, temp, and mem_util — most-severe class wins. Per
    // instance the keys are e.g. `gpu_util` / `gpu_util:amdgpu`, but for
    // simplicity v1 we only look up the unprefixed metric keys. Custom
    // per-vendor thresholds can come later if anyone asks.
    let util_class = gpu
        .util
        .and_then(|u| thresholds::resolve(config, None, "gpu_util", u));
    let temp_class = gpu
        .temp_c
        .and_then(|t| thresholds::resolve(config, None, "gpu_temp", t));
    let mem_class = gpu
        .mem_util
        .and_then(|m| thresholds::resolve(config, None, "gpu_mem_util", m));
    let _ = instance; // reserved for future per-vendor overrides
    thresholds::most_severe([util_class, temp_class, mem_class])
}

fn default_icon(kind: &IndicatorKind) -> &'static str {
    match kind {
        IndicatorKind::Cpu => "applications-system-symbolic",
        IndicatorKind::Mem => "drive-harddisk-system-symbolic",
        IndicatorKind::Swap => "drive-harddisk-symbolic",
        IndicatorKind::Disk { .. } => "drive-harddisk-symbolic",
        IndicatorKind::Net { .. } => "network-wired-symbolic",
        IndicatorKind::Temp { .. } => "temperature-symbolic",
        IndicatorKind::Amdgpu | IndicatorKind::Nvidia => "video-display-symbolic",
    }
}

#[tokio::main]
async fn main() -> glimpse_sdk::AppletResult<()> {
    run(SysmonitorApplet, State::default()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samplers::{CpuSample, MemSample};

    fn sample_with_cpu(util: f64) -> Sample {
        Sample {
            cpu: CpuSample {
                util,
                ..Default::default()
            },
            ..Sample::default()
        }
    }

    /// End-to-end: indicator config → format engine → StatusItem with
    /// label and threshold class applied. This is the user-visible
    /// contract; if it breaks the panel renders nonsense.
    #[test]
    fn cpu_indicator_emits_label_and_threshold_class() {
        let config = Config {
            indicators: vec![IndicatorConfig {
                kind: IndicatorKind::Cpu,
                id: None,
                icon: None,
                label: Some("CPU {cpu_util_pct:.0}%".into()),
                tooltip: None,
                hide_when: None,
            }],
            thresholds: [(
                "cpu_util".to_string(),
                crate::config::ThresholdConfig {
                    warn: 0.75,
                    crit: 0.90,
                },
            )]
            .into_iter()
            .collect(),
            ..Config::default()
        };
        let sample = sample_with_cpu(0.92);
        let item = render_indicator(&config.indicators[0], &sample, &config).unwrap();
        assert_eq!(item.id.as_deref(), Some("cpu"));
        assert_eq!(item.label.as_deref(), Some("CPU 92%"));
        assert_eq!(item.css_classes, vec!["threshold-crit".to_string()]);
    }

    /// Indicators whose underlying sample data is absent return `None`
    /// — disk without an entry in `sample.disks`, GPU without a sample,
    /// etc. — so the panel slot stays clean instead of showing a stub.
    #[test]
    fn indicator_without_sample_data_returns_none() {
        let config = Config {
            indicators: vec![IndicatorConfig {
                kind: IndicatorKind::Disk {
                    mountpoint: "/".into(),
                },
                id: None,
                icon: None,
                label: Some("/".into()),
                tooltip: None,
                hide_when: None,
            }],
            ..Config::default()
        };
        let sample = Sample::default();
        let item = render_indicator(&config.indicators[0], &sample, &config);
        assert!(item.is_none(), "no disk sample → no indicator");
    }

    /// Explicit `id` on an indicator overrides the kind-derived id. Used
    /// by the default config so two `kind = "cpu"` indicators (util and
    /// temp) get distinct panel widget keys.
    #[test]
    fn explicit_id_wins_over_kind_derived_id() {
        let config = Config {
            indicators: vec![IndicatorConfig {
                kind: IndicatorKind::Cpu,
                id: Some("cpu-temp".into()),
                icon: None,
                label: Some("{cpu_temp:.0}°C".into()),
                tooltip: None,
                hide_when: None,
            }],
            ..Config::default()
        };
        let sample = Sample {
            cpu: crate::samplers::CpuSample {
                temp_c: Some(72.0),
                ..Default::default()
            },
            ..Sample::default()
        };
        let item = render_indicator(&config.indicators[0], &sample, &config).unwrap();
        assert_eq!(item.id.as_deref(), Some("cpu-temp"));
        assert_eq!(item.label.as_deref(), Some("72°C"));
    }

    /// The default indicator set has three pills with three distinct
    /// stable ids so the shell can update each in place across ticks.
    /// If anyone re-uses an id, two indicators would collapse onto one
    /// panel widget and confuse the renderer.
    #[test]
    fn default_indicators_have_unique_ids() {
        let defaults = crate::config::default_indicators();
        assert_eq!(defaults.len(), 3, "expected three default indicators");
        let mut ids: Vec<String> = defaults
            .iter()
            .map(|i| i.id.clone().unwrap_or_else(|| i.kind.id()))
            .collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 3, "default indicator ids must be unique");
    }

    /// Default thresholds cover the three metrics the default indicators
    /// surface. Without these, the defaults would never tint warn/crit
    /// even though they're the obvious cases to colour.
    #[test]
    fn default_thresholds_cover_default_indicator_metrics() {
        let defaults = crate::config::default_thresholds();
        for key in &["cpu_util", "mem_util", "cpu_temp"] {
            assert!(defaults.contains_key(*key), "missing default threshold {key}");
        }
    }

    /// Swap indicator with low util produces no threshold class.
    #[test]
    fn swap_indicator_below_threshold_has_no_class() {
        let config = Config {
            indicators: vec![IndicatorConfig {
                kind: IndicatorKind::Swap,
                id: None,
                icon: None,
                label: Some("swap {swap_util_pct:.0}%".into()),
                tooltip: None,
                hide_when: None,
            }],
            thresholds: [(
                "swap_util".to_string(),
                crate::config::ThresholdConfig {
                    warn: 0.50,
                    crit: 0.80,
                },
            )]
            .into_iter()
            .collect(),
            ..Config::default()
        };
        let sample = Sample {
            swap: MemSample {
                total_bytes: 1000,
                used_bytes: 100,
                util: 0.1,
                ..Default::default()
            },
            ..Sample::default()
        };
        let item = render_indicator(&config.indicators[0], &sample, &config).unwrap();
        assert_eq!(item.label.as_deref(), Some("swap 10%"));
        assert!(
            item.css_classes.is_empty(),
            "10% should be under 50% warn threshold"
        );
    }
}
