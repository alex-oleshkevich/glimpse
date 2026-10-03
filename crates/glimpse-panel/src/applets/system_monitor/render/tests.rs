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
fn a_gpu_absent_state_produces_no_gpu_chip_or_row() {
    let state = State::default();

    assert!(
        chips(&state, &[Chip::Gpu], FORMAT, 85, 95).is_empty(),
        "AC-3: no Gpu-derived chip"
    );
    let readings = readings(&state, 85, 95);
    assert!(
        readings.gpu.is_none() && readings.gpu_memory.is_none() && readings.gpu_temp.is_none(),
        "AC-3: no Gpu-derived row"
    );
}

#[test]
fn a_disk_with_no_usage_produces_no_row() {
    let mut state = State::default();
    state.disks.push(glimpse_services::SystemMonitorDiskUsage {
        path: "/mnt/gone".to_owned(),
        usage: None,
    });

    assert!(
        readings(&state, 85, 95).disks.is_empty(),
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
fn no_swap_produces_no_swap_chip_or_row() {
    let state = State {
        cpu: Some(Cpu { percent: 10.0 }),
        ..State::default()
    };

    assert!(chips(&state, &[Chip::Swap], FORMAT, 85, 95).is_empty());
    assert!(readings(&state, 85, 95).swap.is_none());
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
fn a_configured_disk_path_becomes_its_own_row_keyed_by_path() {
    let mut state = State::default();
    state.disks.push(glimpse_services::SystemMonitorDiskUsage {
        path: "/home".to_owned(),
        usage: Some(usage(870_000_000_000, 1_000_000_000_000)),
    });

    let disks = readings(&state, 85, 95).disks;
    assert_eq!(disks.len(), 1);
    assert_eq!(disks[0].title, "/home");
    assert_eq!(disks[0].meter.severity, Some(Severity::Warning));
}

#[test]
fn gpu_memory_row_is_titled_by_its_measured_kind() {
    let state = State {
        gpu: Some(Gpu {
            usage_percent: None,
            memory: usage(1, 2),
            memory_kind: GpuMemoryKind::Gtt,
            temp_c: None,
        }),
        ..State::default()
    };

    let readings = readings(&state, 85, 95);
    assert_eq!(
        readings.gpu_memory.map(|memory| memory.title).as_deref(),
        Some("GTT")
    );
    assert!(
        readings.gpu.is_none(),
        "usage_percent is None, so no separate usage row appears"
    );
}

#[test]
fn details_read_load_average_and_uptime() {
    let state = State {
        load_average: Some(LoadAverage {
            one: 1.24,
            five: 0.98,
            fifteen: 0.87,
        }),
        uptime: Some(Duration::from_secs(2 * 86400 + 4 * 3600)),
        ..State::default()
    };

    let readings = readings(&state, 85, 95);
    assert_eq!(readings.load.as_deref(), Some("1.24, 0.98, 0.87"));
    assert_eq!(readings.uptime.as_deref(), Some("2d 4h"));
}

#[test]
fn a_network_reading_shows_both_directions() {
    let state = State {
        network: Some(NetworkRate {
            rx_bytes_per_sec: 1_200_000.0,
            tx_bytes_per_sec: 84_000.0,
        }),
        ..State::default()
    };

    let network = readings(&state, 85, 95).network.unwrap_or_default();
    assert!(network.contains('↓') && network.contains('↑'));
}
