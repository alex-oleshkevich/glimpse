use std::time::Duration;

use gettextrs::gettext;
use glimpse_config::SystemMonitorChip as Chip;
use glimpse_services::{
    SystemMonitorGpuMemoryKind as GpuMemoryKind, SystemMonitorState as State,
    SystemMonitorUsage as Usage,
};
use glimpse_widgets::{
    IndicatorSpec, Severity, SystemMonitorMeter as Meter, SystemMonitorNamedMeter as NamedMeter,
    SystemMonitorReadings as Readings,
};

use glimpse_utils::size::bytes;

const CHIP_CLASS: &str = "system-monitor-chip";

fn severity_of(percent: f32, warn_percent: u8, critical_percent: u8) -> Option<Severity> {
    if percent >= f32::from(critical_percent) {
        Some(Severity::Error)
    } else if percent >= f32::from(warn_percent) {
        Some(Severity::Warning)
    } else {
        None
    }
}

fn percent_of(usage: Usage) -> f32 {
    (usage.used as f64 / usage.total as f64 * 100.0) as f32
}

fn value_text(usage: Usage) -> String {
    format!("{} / {}", bytes(usage.used), bytes(usage.total))
}

fn rate_text(rate: f64) -> String {
    format!("{}/s", bytes(rate.max(0.0) as u64))
}

fn uptime_text(uptime: Duration) -> String {
    let minutes = uptime.as_secs() / 60;
    let days = minutes / (24 * 60);
    let hours = (minutes / 60) % 24;
    let remaining_minutes = minutes % 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {remaining_minutes}m")
    } else {
        format!("{remaining_minutes}m")
    }
}

pub fn chips(
    state: &State,
    wanted: &[Chip],
    format: &str,
    warn_percent: u8,
    critical_percent: u8,
) -> Vec<IndicatorSpec> {
    wanted
        .iter()
        .filter_map(|chip| chip_spec(state, *chip, format, warn_percent, critical_percent))
        .collect()
}

fn chip_spec(
    state: &State,
    chip: Chip,
    format: &str,
    warn_percent: u8,
    critical_percent: u8,
) -> Option<IndicatorSpec> {
    let (name, value, percent) = match chip {
        Chip::Cpu => {
            let cpu = state.cpu?;
            (
                gettext("CPU"),
                format!("{:.0}%", cpu.percent),
                Some(cpu.percent),
            )
        }
        Chip::Ram => {
            let memory = state.memory.filter(|memory| memory.total > 0)?;
            let percent = percent_of(memory);
            (gettext("RAM"), format!("{percent:.0}%"), Some(percent))
        }
        Chip::Swap => {
            let swap = state.swap.filter(|swap| swap.total > 0)?;
            let percent = percent_of(swap);
            (gettext("Swap"), format!("{percent:.0}%"), Some(percent))
        }
        Chip::Network => {
            let network = state.network?;
            (
                gettext("Net"),
                format!("↓{}", rate_text(network.rx_bytes_per_sec)),
                None,
            )
        }
        Chip::Gpu => {
            let percent = state.gpu.and_then(|gpu| gpu.usage_percent)?;
            (gettext("GPU"), format!("{percent:.0}%"), Some(percent))
        }
        Chip::CpuTemp => {
            let temperature = state.cpu_temperature?;
            (gettext("CPU"), format!("{temperature:.0}°C"), None)
        }
        Chip::GpuTemp => {
            let temperature = state.gpu.and_then(|gpu| gpu.temp_c)?;
            (gettext("GPU"), format!("{temperature:.0}°C"), None)
        }
    };
    let label = crate::applets::tokens::render(format, |token| match token {
        "name" => Some(name.as_str()),
        "value" => Some(value.as_str()),
        _ => None,
    });
    Some(IndicatorSpec {
        label: Some(label),
        severity: percent.and_then(|percent| severity_of(percent, warn_percent, critical_percent)),
        class: Some(CHIP_CLASS.to_owned()),
        ..Default::default()
    })
}

pub fn readings(state: &State, warn_percent: u8, critical_percent: u8) -> Readings {
    let percent = |percent: f32| Meter {
        level: f64::from(percent) / 100.0,
        value: format!("{percent:.0}%"),
        severity: severity_of(percent, warn_percent, critical_percent),
    };
    let meter = |usage: Usage| Meter {
        level: usage.used as f64 / usage.total as f64,
        value: value_text(usage),
        severity: severity_of(percent_of(usage), warn_percent, critical_percent),
    };
    let measured = |usage: &Usage| usage.total > 0;
    let degrees = |temperature: f32| format!("{temperature:.0}°C");
    let gpu = state.gpu;

    Readings {
        cpu: state.cpu.map(|cpu| percent(cpu.percent)),
        memory: state.memory.filter(measured).map(meter),
        swap: state.swap.filter(measured).map(meter),
        disks: state
            .disks
            .iter()
            .filter_map(|disk| {
                disk.usage.filter(measured).map(|usage| NamedMeter {
                    title: disk.path.clone(),
                    meter: meter(usage),
                })
            })
            .collect(),
        gpu: gpu.and_then(|gpu| gpu.usage_percent).map(percent),
        gpu_memory: gpu
            .filter(|gpu| measured(&gpu.memory))
            .map(|gpu| NamedMeter {
                title: match gpu.memory_kind {
                    GpuMemoryKind::Vram => "VRAM".to_owned(),
                    GpuMemoryKind::Gtt => "GTT".to_owned(),
                },
                meter: meter(gpu.memory),
            }),
        load: state
            .load_average
            .map(|load| format!("{:.2}, {:.2}, {:.2}", load.one, load.five, load.fifteen)),
        uptime: state.uptime.map(uptime_text),
        network: state.network.map(|network| {
            format!(
                "↓ {} ↑ {}",
                rate_text(network.rx_bytes_per_sec),
                rate_text(network.tx_bytes_per_sec)
            )
        }),
        cpu_temp: state.cpu_temperature.map(degrees),
        gpu_temp: gpu.and_then(|gpu| gpu.temp_c).map(degrees),
    }
}

#[cfg(test)]
mod tests;
