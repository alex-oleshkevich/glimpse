use std::path::{Path, PathBuf};

use super::{DiskUsage, Gpu, GpuMemoryKind, LoadAverage, Usage};

const PROC_ROOT: &str = "/proc";
const SYS_NET: &str = "/sys/class/net";
const DRM_ROOT: &str = "/sys/class/drm";
const HWMON_ROOT: &str = "/sys/class/hwmon";
const CPU_TEMP_CHIPS: &[&str] = &["k10temp", "zenpower", "coretemp", "cpu_thermal"];
const AMD_VENDOR: &str = "0x1002";

#[derive(Debug, Clone, Default)]
pub struct Sampled {
    pub cpu_total: Option<u64>,
    pub cpu_idle: Option<u64>,
    pub load_average: Option<LoadAverage>,
    pub memory: Option<Usage>,
    pub swap: Option<Usage>,
    pub uptime: Option<std::time::Duration>,
    pub network: Vec<(String, u64, u64)>,
    pub cpu_temperature: Option<f32>,
    pub gpu: Option<Gpu>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Discovery {
    pub gpu: Option<GpuInfo>,
    pub cpu_temp: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GpuInfo {
    busy_percent: PathBuf,
    vram_total: PathBuf,
    vram_used: PathBuf,
    gtt_total: PathBuf,
    gtt_used: PathBuf,
    runtime_status: PathBuf,
    temp: Option<PathBuf>,
    memory_kind: GpuMemoryKind,
}

pub(super) fn discover(gpu_enabled: bool) -> Discovery {
    Discovery {
        gpu: gpu_enabled.then(discover_gpu).flatten(),
        cpu_temp: discover_cpu_temp(),
    }
}

fn discover_cpu_temp() -> Option<PathBuf> {
    let mut chips: Vec<PathBuf> = std::fs::read_dir(HWMON_ROOT)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .collect();
    chips.sort();

    for chip in chips {
        let Ok(name) = std::fs::read_to_string(chip.join("name")) else {
            continue;
        };
        if !CPU_TEMP_CHIPS.contains(&name.trim()) {
            continue;
        }
        let temp = chip.join("temp1_input");
        if temp.exists() {
            return Some(temp);
        }
    }
    None
}

fn discover_gpu() -> Option<GpuInfo> {
    let mut cards: Vec<(u32, PathBuf)> = std::fs::read_dir(DRM_ROOT)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name();
            let index = name.to_str()?.strip_prefix("card")?.parse::<u32>().ok()?;
            Some((index, entry.path().join("device")))
        })
        .collect();
    cards.sort_by_key(|(index, _)| *index);

    for (_, device) in cards {
        let Ok(vendor) = std::fs::read_to_string(device.join("vendor")) else {
            continue;
        };
        if vendor.trim() != AMD_VENDOR {
            continue;
        }
        let busy_percent = device.join("gpu_busy_percent");
        if !busy_percent.exists() {
            continue;
        }

        let vram_total_path = device.join("mem_info_vram_total");
        let gtt_total_path = device.join("mem_info_gtt_total");
        let vram_total = read_u64(&vram_total_path).unwrap_or(0);
        let gtt_total = read_u64(&gtt_total_path).unwrap_or(0);
        let memory_kind = if vram_total.saturating_mul(4) < gtt_total {
            GpuMemoryKind::Gtt
        } else {
            GpuMemoryKind::Vram
        };

        return Some(GpuInfo {
            busy_percent,
            vram_total: vram_total_path,
            vram_used: device.join("mem_info_vram_used"),
            gtt_total: gtt_total_path,
            gtt_used: device.join("mem_info_gtt_used"),
            runtime_status: device.join("power/runtime_status"),
            temp: discover_gpu_temp(&device),
            memory_kind,
        });
    }
    None
}

fn discover_gpu_temp(device: &Path) -> Option<PathBuf> {
    std::fs::read_dir(device.join("hwmon"))
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path().join("temp1_input"))
        .find(|temp| temp.exists())
}

pub(super) fn sample_proc_and_sysfs(discovery: &Discovery) -> Sampled {
    let mut sampled = Sampled::default();

    if let Ok(text) = std::fs::read_to_string(format!("{PROC_ROOT}/stat"))
        && let Some((total, idle)) = parse_stat(&text)
    {
        sampled.cpu_total = Some(total);
        sampled.cpu_idle = Some(idle);
    }
    if let Ok(text) = std::fs::read_to_string(format!("{PROC_ROOT}/meminfo")) {
        let (memory, swap) = parse_meminfo(&text);
        sampled.memory = memory;
        sampled.swap = swap;
    }
    if let Ok(text) = std::fs::read_to_string(format!("{PROC_ROOT}/loadavg")) {
        sampled.load_average = parse_loadavg(&text);
    }
    if let Ok(text) = std::fs::read_to_string(format!("{PROC_ROOT}/uptime")) {
        sampled.uptime = parse_uptime(&text);
    }
    if let Ok(text) = std::fs::read_to_string(format!("{PROC_ROOT}/net/dev")) {
        sampled.network = parse_net_dev(&text)
            .into_iter()
            .filter(|(name, ..)| is_physical_interface(name))
            .collect();
    }
    if let Some(path) = &discovery.cpu_temp {
        sampled.cpu_temperature = read_millidegrees(path);
    }
    sampled.gpu = discovery.gpu.as_ref().and_then(sample_gpu);

    sampled
}

fn sample_gpu(info: &GpuInfo) -> Option<Gpu> {
    let suspended = std::fs::read_to_string(&info.runtime_status)
        .is_ok_and(|status| status.trim() == "suspended");
    let usage_percent = (!suspended).then(|| read_f32(&info.busy_percent)).flatten();

    let (total_path, used_path) = match info.memory_kind {
        GpuMemoryKind::Vram => (&info.vram_total, &info.vram_used),
        GpuMemoryKind::Gtt => (&info.gtt_total, &info.gtt_used),
    };
    let total = read_u64(total_path).unwrap_or(0);
    let used = read_u64(used_path).unwrap_or(0);

    Some(Gpu {
        usage_percent,
        memory: Usage { used, total },
        memory_kind: info.memory_kind,
        temp_c: info.temp.as_deref().and_then(read_millidegrees),
    })
}

fn is_physical_interface(name: &str) -> bool {
    Path::new(SYS_NET).join(name).join("device").exists()
}

fn read_u64(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_f32(path: &Path) -> Option<f32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_millidegrees(path: &Path) -> Option<f32> {
    read_f32(path).map(|millidegrees| millidegrees / 1000.0)
}

pub(super) fn sample_disks(paths: &[PathBuf]) -> Vec<DiskUsage> {
    paths
        .iter()
        .map(|path| DiskUsage {
            path: path.display().to_string(),
            usage: statvfs_usage(path),
        })
        .collect()
}

fn statvfs_usage(path: &Path) -> Option<Usage> {
    let stat = rustix::fs::statvfs(path).ok()?;
    let total = stat.f_blocks.checked_mul(stat.f_frsize)?;
    let free = stat.f_bfree.checked_mul(stat.f_frsize)?;
    Some(Usage {
        used: total.saturating_sub(free),
        total,
    })
}

fn parse_stat(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().find(|line| line.starts_with("cpu "))?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|field| field.parse().ok())
        .collect();
    if fields.len() < 8 {
        return None;
    }
    let total: u64 = fields[..8].iter().sum();
    let idle = fields[3] + fields[4];
    Some((total, idle))
}

fn parse_meminfo(text: &str) -> (Option<Usage>, Option<Usage>) {
    let mut total_kb = None;
    let mut available_kb = None;
    let mut swap_total_kb = None;
    let mut swap_free_kb = None;
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let value = rest
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<u64>().ok());
        match key {
            "MemTotal" => total_kb = value,
            "MemAvailable" => available_kb = value,
            "SwapTotal" => swap_total_kb = value,
            "SwapFree" => swap_free_kb = value,
            _ => {}
        }
    }

    let memory = total_kb.zip(available_kb).map(|(total, available)| Usage {
        total: total * 1024,
        used: total.saturating_sub(available) * 1024,
    });
    let swap = match swap_total_kb {
        Some(0) | None => None,
        Some(total) => swap_free_kb.map(|free| Usage {
            total: total * 1024,
            used: total.saturating_sub(free) * 1024,
        }),
    };
    (memory, swap)
}

fn parse_loadavg(text: &str) -> Option<LoadAverage> {
    let mut fields = text.split_whitespace();
    Some(LoadAverage {
        one: fields.next()?.parse().ok()?,
        five: fields.next()?.parse().ok()?,
        fifteen: fields.next()?.parse().ok()?,
    })
}

fn parse_uptime(text: &str) -> Option<std::time::Duration> {
    let seconds: f64 = text.split_whitespace().next()?.parse().ok()?;
    (seconds >= 0.0).then(|| std::time::Duration::from_secs_f64(seconds))
}

fn parse_net_dev(text: &str) -> Vec<(String, u64, u64)> {
    text.lines()
        .skip(2)
        .filter_map(|line| {
            let (name, rest) = line.split_once(':')?;
            let fields: Vec<u64> = rest
                .split_whitespace()
                .filter_map(|field| field.parse().ok())
                .collect();
            let rx = *fields.first()?;
            let tx = *fields.get(8)?;
            Some((name.trim().to_owned(), rx, tx))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &str = "cpu  28153468 1775111 9605921 167807897 82624623 658118 372139 0 0 0\ncpu0 1015079 70502 308772 5380092 5322960 22412 68151 0 0 0\n";

    #[test]
    fn the_aggregate_cpu_line_sums_the_first_eight_fields() {
        let (total, idle) = parse_stat(STAT).expect("the fixture has a cpu line");
        assert_eq!(
            total,
            28153468 + 1775111 + 9605921 + 167807897 + 82624623 + 658118 + 372139
        );
        assert_eq!(idle, 167807897 + 82624623);
    }

    #[test]
    fn a_missing_cpu_line_yields_nothing() {
        assert_eq!(parse_stat("cpu0 1 2 3 4 5 6 7 8\n"), None);
    }

    const MEMINFO: &str = "MemTotal:       31938016 kB\nMemFree:         6400372 kB\nMemAvailable:   10527312 kB\nSwapTotal:      16777208 kB\nSwapFree:            948 kB\n";

    #[test]
    fn memory_and_swap_are_derived_from_total_minus_available_or_free() {
        let (memory, swap) = parse_meminfo(MEMINFO);
        let memory = memory.expect("both fields are in the fixture");
        assert_eq!(memory.total, 31938016 * 1024);
        assert_eq!(memory.used, (31938016 - 10527312) * 1024);
        let swap = swap.expect("swap total is nonzero");
        assert_eq!(swap.total, 16777208 * 1024);
        assert_eq!(swap.used, (16777208 - 948) * 1024);
    }

    #[test]
    fn a_swap_total_of_zero_is_no_swap_at_all() {
        let (_, swap) = parse_meminfo(
            "MemTotal: 1000 kB\nMemAvailable: 500 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n",
        );
        assert_eq!(swap, None);
    }

    #[test]
    fn loadavg_reads_the_first_three_fields() {
        let load = parse_loadavg("6.29 7.46 9.76 7/4534 3354039\n").expect("parses");
        assert_eq!(load.one, 6.29);
        assert_eq!(load.five, 7.46);
        assert_eq!(load.fifteen, 9.76);
    }

    #[test]
    fn uptime_reads_the_first_field_as_seconds() {
        let uptime = parse_uptime("202031.70 1678079.01\n").expect("parses");
        assert_eq!(uptime, std::time::Duration::from_secs_f64(202031.70));
    }

    const NET_DEV: &str = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 100 1    0    0    0     0          0         0 100 1    0    0    0     0       0          0\nwlp99s0: 8973687366 11666884    0 11029    0     0          0         0 15041670643 16976278    0   35    0     0       0          0\n";

    #[test]
    fn every_interface_line_yields_its_rx_and_tx_byte_counters() {
        let interfaces = parse_net_dev(NET_DEV);
        assert_eq!(
            interfaces,
            vec![
                ("lo".to_owned(), 100, 100),
                ("wlp99s0".to_owned(), 8973687366, 15041670643),
            ]
        );
    }

    #[test]
    fn a_disappearing_interface_is_simply_absent_from_the_second_parse() {
        let first = parse_net_dev(NET_DEV);
        let second = parse_net_dev(
            "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 200 2    0    0    0     0          0         0 200 2    0    0    0     0       0          0\n",
        );
        assert_eq!(first.len(), 2);
        assert_eq!(second, vec![("lo".to_owned(), 200, 200)]);
    }
}
