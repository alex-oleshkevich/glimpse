use std::time::Duration;

use gettextrs::gettext;
use glimpse_config::SystemMonitorChip as Chip;
use glimpse_services::{
    SystemMonitorGpuMemoryKind as GpuMemoryKind, SystemMonitorState as State,
    SystemMonitorUsage as Usage,
};
use glimpse_widgets::{
    IndicatorSpec, Severity, SystemMonitorDetail as DetailTile, SystemMonitorUsage as UsageTile,
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

pub fn usage_tiles(state: &State, warn_percent: u8, critical_percent: u8) -> Vec<UsageTile> {
    let mut tiles = Vec::new();

    if let Some(cpu) = state.cpu {
        tiles.push(UsageTile {
            id: "cpu".to_owned(),
            title: gettext("CPU"),
            value: format!("{:.0}%", cpu.percent),
            fraction: Some(f64::from(cpu.percent) / 100.0),
            severity: severity_of(cpu.percent, warn_percent, critical_percent),
        });
    }
    if let Some(memory) = state.memory.filter(|memory| memory.total > 0) {
        tiles.push(usage_tile(
            "memory",
            gettext("Memory"),
            memory,
            warn_percent,
            critical_percent,
        ));
    }
    if let Some(swap) = state.swap.filter(|swap| swap.total > 0) {
        tiles.push(usage_tile(
            "swap",
            gettext("Swap"),
            swap,
            warn_percent,
            critical_percent,
        ));
    }
    for disk in &state.disks {
        let Some(usage) = disk.usage.filter(|usage| usage.total > 0) else {
            continue;
        };
        tiles.push(usage_tile(
            &disk.path,
            disk.path.clone(),
            usage,
            warn_percent,
            critical_percent,
        ));
    }
    if let Some(gpu) = state.gpu {
        if let Some(usage_percent) = gpu.usage_percent {
            tiles.push(UsageTile {
                id: "gpu-usage".to_owned(),
                title: gettext("GPU"),
                value: format!("{usage_percent:.0}%"),
                fraction: Some(f64::from(usage_percent) / 100.0),
                severity: severity_of(usage_percent, warn_percent, critical_percent),
            });
        }
        if gpu.memory.total > 0 {
            let title = match gpu.memory_kind {
                GpuMemoryKind::Vram => "VRAM".to_owned(),
                GpuMemoryKind::Gtt => "GTT".to_owned(),
            };
            tiles.push(usage_tile(
                "gpu-memory",
                title,
                gpu.memory,
                warn_percent,
                critical_percent,
            ));
        }
    }

    tiles
}

fn usage_tile(
    id: &str,
    title: String,
    usage: Usage,
    warn_percent: u8,
    critical_percent: u8,
) -> UsageTile {
    let fraction = usage.used as f64 / usage.total as f64;
    let percent = (fraction * 100.0) as f32;
    UsageTile {
        id: id.to_owned(),
        title,
        value: value_text(usage),
        fraction: Some(fraction),
        severity: severity_of(percent, warn_percent, critical_percent),
    }
}

pub fn detail_tiles(state: &State) -> Vec<DetailTile> {
    let mut tiles = Vec::new();

    if let Some(load) = state.load_average {
        tiles.push(DetailTile {
            id: "load-average".to_owned(),
            title: gettext("Load average"),
            value: format!("{:.2}, {:.2}, {:.2}", load.one, load.five, load.fifteen),
        });
    }
    if let Some(uptime) = state.uptime {
        tiles.push(DetailTile {
            id: "uptime".to_owned(),
            title: gettext("Uptime"),
            value: uptime_text(uptime),
        });
    }
    if let Some(network) = state.network {
        tiles.push(DetailTile {
            id: "network".to_owned(),
            title: gettext("Network"),
            value: format!(
                "↓ {} ↑ {}",
                rate_text(network.rx_bytes_per_sec),
                rate_text(network.tx_bytes_per_sec)
            ),
        });
    }
    if let Some(temperature) = state.cpu_temperature {
        tiles.push(DetailTile {
            id: "cpu-temp".to_owned(),
            title: gettext("CPU temperature"),
            value: format!("{temperature:.0}°C"),
        });
    }
    if let Some(temperature) = state.gpu.and_then(|gpu| gpu.temp_c) {
        tiles.push(DetailTile {
            id: "gpu-temp".to_owned(),
            title: gettext("GPU temperature"),
            value: format!("{temperature:.0}°C"),
        });
    }

    tiles
}

#[cfg(test)]
mod tests;
