use std::fs;

/// Per-tick CPU snapshot. All optional fields fall back to neutral values
/// (`0.0`, `None`) when the kernel doesn't expose them on this host — e.g.
/// `cpu MHz` is missing on some ARM kernels, `coretemp` is missing if no
/// hwmon driver is loaded.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuSample {
    /// Aggregate utilization across all cores, `0.0..=1.0`. `0.0` on the
    /// first tick (no prior reading to delta against).
    pub util: f64,
    /// Average current frequency across cores, in MHz.
    pub freq_mhz: f64,
    /// Package temperature in °C if a coretemp / k10temp / zenpower hwmon
    /// is present; `None` otherwise. Renderers should hide the token when
    /// `None` rather than print a misleading 0°C.
    pub temp_c: Option<f64>,
    pub cores: usize,
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
}

#[derive(Debug, Clone, Copy)]
struct StatSnapshot {
    /// Sum of all `cpu` columns from /proc/stat.
    total: u64,
    /// "Busy" time, defined here as `total - idle - iowait`. iowait is
    /// excluded deliberately: it counts time the CPU was idle waiting on
    /// I/O, which most monitoring tools (htop, glances) do not consider
    /// active utilization. Including it would inflate `cpu_util` whenever
    /// disk pressure builds up, even on an otherwise idle machine.
    busy: u64,
}

#[derive(Debug, Default)]
pub struct CpuSampler {
    cores: usize,
    prev: Option<StatSnapshot>,
}

impl CpuSampler {
    pub fn new() -> Self {
        Self {
            cores: read_core_count().unwrap_or(1),
            prev: None,
        }
    }

    pub fn tick(&mut self) -> CpuSample {
        let stat = read_proc_stat().ok();
        let util = match (&stat, &self.prev) {
            (Some(now), Some(prev)) => util_from_delta(prev, now),
            _ => 0.0,
        };
        if let Some(s) = stat {
            self.prev = Some(s);
        }
        let (load_1, load_5, load_15) = read_loadavg().unwrap_or((0.0, 0.0, 0.0));
        CpuSample {
            util,
            freq_mhz: read_avg_freq_mhz().unwrap_or(0.0),
            temp_c: read_package_temp_c(),
            cores: self.cores,
            load_1,
            load_5,
            load_15,
        }
    }
}

fn util_from_delta(prev: &StatSnapshot, now: &StatSnapshot) -> f64 {
    // saturating_sub so a /proc/stat counter wrap (rare but possible across
    // suspend/resume) clamps to zero instead of producing a wildly negative
    // delta that would then divide into a nonsense f64.
    let dt = now.total.saturating_sub(prev.total);
    let db = now.busy.saturating_sub(prev.busy);
    if dt == 0 { 0.0 } else { (db as f64 / dt as f64).clamp(0.0, 1.0) }
}

fn read_proc_stat() -> std::io::Result<StatSnapshot> {
    let contents = fs::read_to_string("/proc/stat")?;
    parse_proc_stat(&contents).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "no `cpu` line in /proc/stat")
    })
}

fn parse_proc_stat(contents: &str) -> Option<StatSnapshot> {
    let line = contents.lines().next()?;
    let mut parts = line.split_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }
    let values: Vec<u64> = parts.filter_map(|v| v.parse().ok()).collect();
    // Columns (kernel >= 2.6.33): user nice system idle iowait irq softirq
    // steal guest guest_nice. Older kernels expose fewer; treat missing as 0.
    if values.len() < 4 {
        return None;
    }
    let total: u64 = values.iter().sum();
    let idle = values[3];
    let iowait = values.get(4).copied().unwrap_or(0);
    Some(StatSnapshot {
        total,
        busy: total.saturating_sub(idle).saturating_sub(iowait),
    })
}

fn read_core_count() -> Option<usize> {
    let info = fs::read_to_string("/proc/cpuinfo").ok()?;
    Some(info.lines().filter(|l| l.starts_with("processor")).count()).filter(|n| *n > 0)
}

fn read_avg_freq_mhz() -> Option<f64> {
    let info = fs::read_to_string("/proc/cpuinfo").ok()?;
    let freqs: Vec<f64> = info
        .lines()
        .filter_map(|line| {
            line.strip_prefix("cpu MHz")
                .and_then(|rhs| rhs.split(':').nth(1))
                .and_then(|val| val.trim().parse::<f64>().ok())
        })
        .collect();
    if freqs.is_empty() {
        None
    } else {
        Some(freqs.iter().sum::<f64>() / freqs.len() as f64)
    }
}

fn read_loadavg() -> Option<(f64, f64, f64)> {
    let raw = fs::read_to_string("/proc/loadavg").ok()?;
    let mut parts = raw.split_whitespace();
    let a = parts.next()?.parse().ok()?;
    let b = parts.next()?.parse().ok()?;
    let c = parts.next()?.parse().ok()?;
    Some((a, b, c))
}

fn read_package_temp_c() -> Option<f64> {
    // Walk /sys/class/hwmon/hwmon*/, prefer drivers known to expose package
    // temperature, then probe their `tempN_label` files for "Package id 0"
    // (Intel coretemp) and fall back to `temp1_input` for AMD k10temp /
    // zenpower (single-sensor packages). Values are millidegrees C.
    const PREFERRED: &[&str] = &["coretemp", "k10temp", "zenpower"];
    let entries = fs::read_dir("/sys/class/hwmon").ok()?;
    let mut candidates: Vec<(usize, std::path::PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match fs::read_to_string(path.join("name")) {
            Ok(s) => s.trim().to_owned(),
            Err(_) => continue,
        };
        if let Some(rank) = PREFERRED.iter().position(|p| *p == name) {
            candidates.push((rank, path));
        }
    }
    candidates.sort_by_key(|(rank, _)| *rank);
    for (_, dir) in candidates {
        if let Some(value) = read_package_temp_from_dir(&dir) {
            return Some(value);
        }
    }
    None
}

fn read_package_temp_from_dir(dir: &std::path::Path) -> Option<f64> {
    // Scan temp*_label for "Package id" first; fall back to temp1_input so
    // single-sensor AMD parts still report something useful.
    for n in 1..=16u32 {
        let label_path = dir.join(format!("temp{n}_label"));
        if let Ok(label) = fs::read_to_string(&label_path)
            && label.trim().starts_with("Package id")
            && let Ok(raw) = fs::read_to_string(dir.join(format!("temp{n}_input")))
            && let Ok(milli) = raw.trim().parse::<i64>()
        {
            return Some(milli as f64 / 1000.0);
        }
    }
    let raw = fs::read_to_string(dir.join("temp1_input")).ok()?;
    let milli: i64 = raw.trim().parse().ok()?;
    Some(milli as f64 / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(values: &[u64]) -> String {
        let mut line = String::from("cpu");
        for v in values {
            line.push(' ');
            line.push_str(&v.to_string());
        }
        line.push('\n');
        line
    }

    /// Pure deltas, no I/O. user=10, idle=90 → busy=10, total=100 → 10%.
    /// idle=80, iowait=10 makes the second tick's idle+iowait=90; busy
    /// stays at 10. Util is the ratio of *deltas*, not raw values.
    #[test]
    fn util_is_ratio_of_busy_delta_to_total_delta() {
        let prev = parse_proc_stat(&snapshot(&[0, 0, 0, 0, 0, 0, 0])).unwrap();
        let next = parse_proc_stat(&snapshot(&[40, 0, 0, 50, 10, 0, 0])).unwrap();
        let util = util_from_delta(&prev, &next);
        // total=100, idle=50, iowait=10 → busy=40 → 0.40
        assert!((util - 0.40).abs() < 1e-9, "util was {util}");
    }

    /// Fully-idle delta yields 0. Important boundary: it's tempting to
    /// silently divide-by-zero here; the implementation must return 0 not NaN.
    #[test]
    fn util_returns_zero_on_no_progress() {
        let prev = parse_proc_stat(&snapshot(&[0, 0, 0, 100, 0, 0, 0])).unwrap();
        let next = parse_proc_stat(&snapshot(&[0, 0, 0, 100, 0, 0, 0])).unwrap();
        let util = util_from_delta(&prev, &next);
        assert!(util.abs() < 1e-9);
    }

    /// Saturated CPU: all busy, no idle progress → 1.0.
    #[test]
    fn util_saturates_at_one() {
        let prev = parse_proc_stat(&snapshot(&[0, 0, 0, 0, 0, 0, 0])).unwrap();
        let next = parse_proc_stat(&snapshot(&[100, 0, 0, 0, 0, 0, 0])).unwrap();
        let util = util_from_delta(&prev, &next);
        assert!((util - 1.0).abs() < 1e-9);
    }

    /// iowait is excluded from busy, by design. This test pins that policy
    /// — flipping it would inflate util whenever the disk is busy on an
    /// otherwise idle machine.
    #[test]
    fn iowait_does_not_count_as_busy() {
        let prev = parse_proc_stat(&snapshot(&[0, 0, 0, 0, 0, 0, 0])).unwrap();
        // user=0, idle=50, iowait=50. Total=100, busy=0, util=0.
        let next = parse_proc_stat(&snapshot(&[0, 0, 0, 50, 50, 0, 0])).unwrap();
        let util = util_from_delta(&prev, &next);
        assert!(util.abs() < 1e-9, "util was {util}, expected 0");
    }

    /// A `/proc/stat` counter wrap (suspend, namespace reset) shouldn't
    /// produce negative deltas — saturating_sub turns them into zero.
    #[test]
    fn counter_wrap_clamps_to_zero_util() {
        let prev = parse_proc_stat(&snapshot(&[200, 0, 0, 800, 0, 0, 0])).unwrap();
        let next = parse_proc_stat(&snapshot(&[10, 0, 0, 50, 0, 0, 0])).unwrap();
        let util = util_from_delta(&prev, &next);
        // total delta saturates to 0 → return 0, not NaN.
        assert!(util.abs() < 1e-9, "util was {util}");
    }
}
