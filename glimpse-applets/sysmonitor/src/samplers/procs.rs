use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// One row in the "Top CPU" / "Top RAM" popover lists.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessSample {
    pub pid: u32,
    /// Real UID; used to hide kill buttons on processes the applet
    /// doesn't own (it would fail with EPERM anyway, but the UI should
    /// be honest about which rows are actionable).
    pub uid: u32,
    pub comm: String,
    /// `1.0 == one full core`. A process saturating 4 cores shows 4.0 —
    /// `htop`'s convention. Format multiplied by 100 for `%` display.
    pub cpu_pct: f64,
    pub rss_bytes: u64,
}

/// Top-N lists produced from a single pass over `/proc`. The same
/// snapshot feeds both lists, so any process appearing in both is
/// guaranteed to have consistent values across the two views.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcTopN {
    pub top_cpu: Vec<ProcessSample>,
    pub top_rss: Vec<ProcessSample>,
}

#[derive(Debug, Default)]
pub struct ProcSampler {
    /// Per-PID running sum of `utime + stime` in kernel ticks at the
    /// last tick. Used to compute CPU% as the delta over total CPU time.
    prev_ticks: HashMap<u32, u64>,
    /// Sum across all CPUs of all /proc/stat columns at the last tick.
    /// Same denominator as htop / top use for "%CPU".
    prev_total: u64,
    page_size: u64,
}

impl ProcSampler {
    pub fn new() -> Self {
        Self {
            prev_ticks: HashMap::new(),
            prev_total: 0,
            page_size: page_size(),
        }
    }

    /// Walk `/proc`, compute per-process %CPU and RSS, return the top
    /// `top_n` rows by each metric. `top_n == 0` short-circuits to an
    /// empty result so the sampler is a no-op when the user hasn't
    /// configured any top-process tile.
    pub fn tick(&mut self, top_n: usize) -> ProcTopN {
        if top_n == 0 {
            return ProcTopN::default();
        }
        let now_total = read_total_cpu_ticks().unwrap_or(0);
        let dt_total = now_total.saturating_sub(self.prev_total);
        self.prev_total = now_total;

        let mut samples = self.scan_proc(dt_total);
        let live: HashSet<u32> = samples.iter().map(|s| s.pid).collect();
        // GC: drop stale PIDs so the prev_ticks map doesn't grow without
        // bound across the applet's lifetime.
        self.prev_ticks.retain(|pid, _| live.contains(pid));

        let mut by_cpu = samples.clone();
        by_cpu.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
        by_cpu.truncate(top_n);

        samples.sort_by_key(|s| std::cmp::Reverse(s.rss_bytes));
        samples.truncate(top_n);

        ProcTopN {
            top_cpu: by_cpu,
            top_rss: samples,
        }
    }

    fn scan_proc(&mut self, dt_total: u64) -> Vec<ProcessSample> {
        let mut out = Vec::new();
        let entries = match fs::read_dir("/proc") {
            Ok(e) => e,
            Err(_) => return out,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(pid) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            let Some(sample) = self.sample_pid(pid, &path, dt_total) else {
                continue;
            };
            out.push(sample);
        }
        out
    }

    fn sample_pid(&mut self, pid: u32, dir: &Path, dt_total: u64) -> Option<ProcessSample> {
        let stat = fs::read_to_string(dir.join("stat")).ok()?;
        let (utime, stime) = parse_stat_times(&stat)?;
        let ticks = utime.saturating_add(stime);
        let prev = self.prev_ticks.get(&pid).copied().unwrap_or(ticks);
        self.prev_ticks.insert(pid, ticks);

        let cpu_pct = if dt_total == 0 {
            0.0
        } else {
            // dt is across *all* CPUs in jiffies, so a single-core process
            // saturating itself maxes at 1/cores per jiffy — multiplying
            // by cores yields 1.0 for one-full-core, matching htop.
            let cores = num_cores();
            let delta = ticks.saturating_sub(prev) as f64;
            (delta / dt_total as f64) * cores as f64
        };

        let comm = fs::read_to_string(dir.join("comm"))
            .ok()
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        let uid = read_uid(dir).unwrap_or(0);
        let rss_bytes = parse_statm_rss(dir).unwrap_or(0) * self.page_size;
        Some(ProcessSample {
            pid,
            uid,
            comm,
            cpu_pct,
            rss_bytes,
        })
    }
}

/// Parses `utime` (col 14) and `stime` (col 15) out of `/proc/<pid>/stat`.
/// The `comm` column is wrapped in `(...)` and can contain arbitrary
/// characters including spaces and parens — we slice on the LAST `)` so
/// rogue comm values can't shift our column indexing.
fn parse_stat_times(stat: &str) -> Option<(u64, u64)> {
    let close = stat.rfind(')')?;
    let rest = stat.get(close + 1..)?.trim();
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After the comm column, fields are:
    //  [0] state, [1] ppid, [2] pgrp, [3] session, [4] tty_nr, [5] tpgid,
    //  [6] flags, [7] minflt, [8] cminflt, [9] majflt, [10] cmajflt,
    //  [11] utime, [12] stime, ...
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some((utime, stime))
}

fn parse_statm_rss(dir: &Path) -> Option<u64> {
    let raw = fs::read_to_string(dir.join("statm")).ok()?;
    // statm columns: size resident shared text lib data dt — in pages.
    raw.split_whitespace().nth(1)?.parse().ok()
}

fn read_uid(dir: &Path) -> Option<u32> {
    let status = fs::read_to_string(dir.join("status")).ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            // Real Effective Saved FS — we want the real UID (first).
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

fn read_total_cpu_ticks() -> Option<u64> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    let line = stat.lines().next()?;
    let mut parts = line.split_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }
    Some(parts.filter_map(|v| v.parse::<u64>().ok()).sum())
}

fn page_size() -> u64 {
    // SAFETY: sysconf is a pure read of a system constant; cannot fail at
    // runtime in a way that produces UB. Returns -1 on unknown selectors
    // (which doesn't apply to _SC_PAGESIZE); we fall back to 4 KiB then.
    let raw = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if raw <= 0 { 4096 } else { raw as u64 }
}

fn num_cores() -> usize {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .map(|c| c.lines().filter(|l| l.starts_with("processor")).count())
        .filter(|n| *n > 0)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real `/proc/<pid>/stat` line with parens AND a space inside `comm`.
    /// Naively splitting on whitespace and indexing field 13 would yield
    /// `stime`, not `utime`. The "find last `)`" trick is what makes the
    /// parser robust against rogue process names.
    #[test]
    fn parse_stat_handles_parens_and_spaces_in_comm() {
        let stat = "12345 (weird (proc name) S 1 12345 12345 0 -1 4194304 1234 0 0 0 17 23 0 0 20 0 1 0 9999999 12345678 100 18446744073709551615 1 1 0 0 0 0 0 0 0 0 0 0 17 1 0 0 0 0 0 0 0 0 0 0 0 0 0\n";
        let (utime, stime) = parse_stat_times(stat).expect("parses");
        assert_eq!(utime, 17);
        assert_eq!(stime, 23);
    }

    /// Standard format with simple comm.
    #[test]
    fn parse_stat_finds_utime_and_stime() {
        let stat = "1 (systemd) S 0 1 1 0 -1 4194560 12345 0 0 0 42 7 0 0 20 0 1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0\n";
        let (utime, stime) = parse_stat_times(stat).expect("parses");
        assert_eq!(utime, 42);
        assert_eq!(stime, 7);
    }

    /// Truncated stat → None, not a panic.
    #[test]
    fn parse_stat_returns_none_when_truncated() {
        assert!(parse_stat_times("123 (foo) S 1 2 3").is_none());
    }

    /// statm RSS is the second column. Off-by-one here would mis-report
    /// every process's memory usage on the panel.
    #[test]
    fn parse_statm_returns_resident_column() {
        // Write a synthetic statm to a tempdir so we exercise the file path.
        let dir = std::env::temp_dir().join(format!(
            "sysmon-statm-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("statm"), "1024 256 64 32 0 200 0\n").unwrap();
        let pages = parse_statm_rss(&dir).expect("parses");
        assert_eq!(pages, 256);
    }
}
