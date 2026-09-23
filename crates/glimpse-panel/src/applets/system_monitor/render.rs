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
mod tests {
    use glimpse_services::{
        SystemMonitorCpu as Cpu, SystemMonitorGpu as Gpu, SystemMonitorLoadAverage as LoadAverage,
        SystemMonitorNetworkRate as NetworkRate,
    };

    use super::*;

    const FORMAT: &str = "{name} {value}";

    fn usage(used: u64, total: u64) -> Usage {
        Usage { used, total }
    }

    #[test]
    fn a_gpu_absent_state_produces_no_gpu_chip_or_tile() {
        let state = State::default();

        assert!(
            chips(&state, &[Chip::Gpu], FORMAT, 85, 95).is_empty(),
            "AC-3: no Gpu-derived chip"
        );
        assert!(
            usage_tiles(&state, 85, 95)
                .iter()
                .all(|tile| tile.id != "gpu-usage" && tile.id != "gpu-memory"),
            "AC-3: no Gpu-derived usage tile"
        );
        assert!(
            detail_tiles(&state)
                .iter()
                .all(|tile| tile.id != "gpu-temp"),
            "AC-3: no Gpu-derived detail tile"
        );
    }

    #[test]
    fn a_disk_with_no_usage_produces_no_tile() {
        let mut state = State::default();
        state.disks.push(glimpse_services::SystemMonitorDiskUsage {
            path: "/mnt/gone".to_owned(),
            usage: None,
        });

        assert!(
            usage_tiles(&state, 85, 95).is_empty(),
            "AC-4a: a path whose usage is None renders no tile"
        );
    }

    #[test]
    fn thresholds_map_below_warn_warn_and_critical_to_the_right_severity() {
        assert_eq!(severity_of(84.0, 85, 95), None, "AC-9");
        assert_eq!(severity_of(85.0, 85, 95), Some(Severity::Warning), "AC-9");
        assert_eq!(severity_of(95.0, 85, 95), Some(Severity::Error), "AC-9");
    }

    #[test]
    fn no_swap_produces_no_swap_chip_or_tile() {
        let state = State {
            cpu: Some(Cpu { percent: 10.0 }),
            ..State::default()
        };

        assert!(chips(&state, &[Chip::Swap], FORMAT, 85, 95).is_empty());
        assert!(
            usage_tiles(&state, 85, 95)
                .iter()
                .all(|tile| tile.id != "swap")
        );
    }

    #[test]
    fn the_cpu_chip_carries_the_measured_percentage_and_no_others_are_shown() {
        let state = State {
            cpu: Some(Cpu { percent: 42.4 }),
            ..State::default()
        };

        let chips = chips(&state, &[Chip::Cpu, Chip::Ram], FORMAT, 85, 95);
        assert_eq!(chips.len(), 1, "ram has no reading yet and must not appear");
        assert_eq!(chips[0].label.as_deref(), Some("CPU 42%"));
    }

    #[test]
    fn a_custom_chip_format_reorders_or_drops_either_token() {
        let state = State {
            cpu: Some(Cpu { percent: 42.4 }),
            ..State::default()
        };

        let value_only = chips(&state, &[Chip::Cpu], "{value}", 85, 95);
        assert_eq!(value_only[0].label.as_deref(), Some("42%"));

        let reordered = chips(&state, &[Chip::Cpu], "{value} ({name})", 85, 95);
        assert_eq!(reordered[0].label.as_deref(), Some("42% (CPU)"));

        let unknown_token = chips(&state, &[Chip::Cpu], "{name}: {nonesuch}", 85, 95);
        assert_eq!(
            unknown_token[0].label.as_deref(),
            Some("CPU: {nonesuch}"),
            "an unrecognized token is left as literal text rather than silently emptied"
        );
    }

    #[test]
    fn a_configured_disk_path_becomes_its_own_tile_keyed_by_path() {
        let mut state = State::default();
        state.disks.push(glimpse_services::SystemMonitorDiskUsage {
            path: "/home".to_owned(),
            usage: Some(usage(870_000_000_000, 1_000_000_000_000)),
        });

        let tiles = usage_tiles(&state, 85, 95);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].id, "/home");
        assert_eq!(tiles[0].severity, Some(Severity::Warning));
    }

    #[test]
    fn gpu_memory_tile_is_titled_by_its_measured_kind() {
        let state = State {
            gpu: Some(Gpu {
                usage_percent: None,
                memory: usage(1, 2),
                memory_kind: GpuMemoryKind::Gtt,
                temp_c: None,
            }),
            ..State::default()
        };

        let tiles = usage_tiles(&state, 85, 95);
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].title, "GTT");
        assert!(
            usage_tiles(&state, 85, 95)
                .iter()
                .all(|tile| tile.id != "gpu-usage"),
            "usage_percent is None, so no separate usage tile appears"
        );
    }

    #[test]
    fn detail_tiles_read_load_average_and_uptime() {
        let state = State {
            load_average: Some(LoadAverage {
                one: 1.24,
                five: 0.98,
                fifteen: 0.87,
            }),
            uptime: Some(Duration::from_secs(2 * 86400 + 4 * 3600)),
            ..State::default()
        };

        let tiles = detail_tiles(&state);
        assert_eq!(tiles[0].value, "1.24, 0.98, 0.87");
        assert_eq!(tiles[1].value, "2d 4h");
    }

    #[test]
    fn a_network_reading_produces_one_detail_tile_with_both_directions() {
        let state = State {
            network: Some(NetworkRate {
                rx_bytes_per_sec: 1_200_000.0,
                tx_bytes_per_sec: 84_000.0,
            }),
            ..State::default()
        };

        let tiles = detail_tiles(&state);
        assert_eq!(tiles.len(), 1);
        assert!(tiles[0].value.contains('↓') && tiles[0].value.contains('↑'));
    }
}
