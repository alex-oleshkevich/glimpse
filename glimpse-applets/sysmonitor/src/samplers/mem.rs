use std::fs;

/// Memory snapshot in bytes. Used for both RAM and swap — the same shape
/// applies because swap has the same `total/free/used` story (just no
/// `available` concept; we fall back to `free` there).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MemSample {
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub avail_bytes: u64,
    pub total_bytes: u64,
    /// Used fraction `0.0..=1.0`. `0.0` when `total_bytes == 0` (swap-less
    /// hosts, or `/proc/meminfo` parse failure on the very first tick).
    pub util: f64,
}

/// Returned by `MemSampler::tick`. Both fields filled from a single read of
/// `/proc/meminfo` so they're always for the same instant in time.
#[derive(Debug, Clone, Default)]
pub struct MemSwap {
    pub mem: MemSample,
    pub swap: MemSample,
}

#[derive(Debug, Default)]
pub struct MemSampler;

impl MemSampler {
    pub fn new() -> Self {
        Self
    }

    pub fn tick(&mut self) -> MemSwap {
        let contents = fs::read_to_string("/proc/meminfo").unwrap_or_default();
        parse_meminfo(&contents)
    }
}

/// Parses `/proc/meminfo`. All values are in KiB (kibibytes) per the kernel
/// docs — we multiply by 1024 to get bytes.
///
/// Used vs. Available: `used = total - available` is what `free(1)`,
/// `htop`, and `glances` show; raw `MemFree` excludes cache/buffers and is
/// misleadingly small on healthy systems. We use the modern definition.
fn parse_meminfo(contents: &str) -> MemSwap {
    let kib = parse_kib_lines(contents);

    let mem_total = kib.get("MemTotal").copied().unwrap_or(0) * 1024;
    let mem_free = kib.get("MemFree").copied().unwrap_or(0) * 1024;
    let mem_avail = kib
        .get("MemAvailable")
        .copied()
        .unwrap_or_else(|| kib.get("MemFree").copied().unwrap_or(0))
        * 1024;
    let mem_used = mem_total.saturating_sub(mem_avail);
    let mem_util = if mem_total == 0 {
        0.0
    } else {
        mem_used as f64 / mem_total as f64
    };

    let swap_total = kib.get("SwapTotal").copied().unwrap_or(0) * 1024;
    let swap_free = kib.get("SwapFree").copied().unwrap_or(0) * 1024;
    let swap_used = swap_total.saturating_sub(swap_free);
    let swap_util = if swap_total == 0 {
        0.0
    } else {
        swap_used as f64 / swap_total as f64
    };

    MemSwap {
        mem: MemSample {
            used_bytes: mem_used,
            free_bytes: mem_free,
            avail_bytes: mem_avail,
            total_bytes: mem_total,
            util: mem_util,
        },
        swap: MemSample {
            used_bytes: swap_used,
            free_bytes: swap_free,
            // Swap has no "available" concept distinct from free.
            avail_bytes: swap_free,
            total_bytes: swap_total,
            util: swap_util,
        },
    }
}

fn parse_kib_lines(contents: &str) -> std::collections::HashMap<&str, u64> {
    let mut out = std::collections::HashMap::new();
    for line in contents.lines() {
        // Format: "MemTotal:       16384000 kB"
        let mut parts = line.splitn(2, ':');
        let key = match parts.next() {
            Some(k) => k.trim(),
            None => continue,
        };
        let rest = match parts.next() {
            Some(r) => r.trim(),
            None => continue,
        };
        // Strip the trailing " kB" if present; everything in meminfo is in
        // KiB regardless, so we don't bother validating the unit suffix.
        let number = rest.split_whitespace().next().unwrap_or("");
        if let Ok(value) = number.parse::<u64>() {
            out.insert(key, value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
MemTotal:       16384000 kB
MemFree:         1024000 kB
MemAvailable:    8192000 kB
Buffers:          512000 kB
Cached:          2048000 kB
SwapTotal:       8388608 kB
SwapFree:        4194304 kB
";

    /// Pins the `used = total - available` definition. If anyone "fixes"
    /// this to `total - free`, mem_used reported on the panel will jump by
    /// the size of the page cache and look terrifying on idle machines.
    #[test]
    fn used_is_total_minus_available() {
        let snap = parse_meminfo(SAMPLE);
        assert_eq!(snap.mem.total_bytes, 16_384_000 * 1024);
        assert_eq!(snap.mem.avail_bytes, 8_192_000 * 1024);
        assert_eq!(snap.mem.used_bytes, (16_384_000 - 8_192_000) * 1024);
        // util = 8M / 16M = 0.5
        assert!((snap.mem.util - 0.5).abs() < 1e-9);
    }

    /// Swap parses the same way but with the no-`SwapAvailable` policy:
    /// `available` falls back to `free`, `used = total - free`.
    #[test]
    fn swap_used_is_total_minus_free() {
        let snap = parse_meminfo(SAMPLE);
        assert_eq!(snap.swap.total_bytes, 8_388_608 * 1024);
        assert_eq!(snap.swap.free_bytes, 4_194_304 * 1024);
        assert_eq!(snap.swap.used_bytes, 4_194_304 * 1024);
        assert!((snap.swap.util - 0.5).abs() < 1e-9);
    }

    /// Swap-less host: SwapTotal=0 must not produce NaN util (division by
    /// zero in f64 is silent and would propagate through every format
    /// token using `swap_util_pct`).
    #[test]
    fn swap_util_is_zero_when_total_is_zero() {
        let raw = "MemTotal: 1000 kB\nMemAvailable: 500 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n";
        let snap = parse_meminfo(raw);
        assert!(snap.swap.util.abs() < f64::EPSILON);
        assert_eq!(snap.swap.total_bytes, 0);
    }

    /// Pre-3.14 kernels without `MemAvailable`: fall back to `MemFree` so
    /// the applet still produces a reasonable (if pessimistic) number
    /// instead of acting like all memory is used.
    #[test]
    fn falls_back_to_memfree_when_memavailable_is_missing() {
        let raw = "MemTotal: 1000 kB\nMemFree: 400 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n";
        let snap = parse_meminfo(raw);
        // avail_bytes uses MemFree, so used = total - free = 600 KiB.
        assert_eq!(snap.mem.avail_bytes, 400 * 1024);
        assert_eq!(snap.mem.used_bytes, 600 * 1024);
    }

    /// Garbage input doesn't panic; everything defaults to zero.
    #[test]
    fn empty_input_yields_zeros() {
        let snap = parse_meminfo("");
        assert_eq!(snap.mem.total_bytes, 0);
        assert!(snap.mem.util.abs() < f64::EPSILON);
        assert!(snap.swap.util.abs() < f64::EPSILON);
    }
}
