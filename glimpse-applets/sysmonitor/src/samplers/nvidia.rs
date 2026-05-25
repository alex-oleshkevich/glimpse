use std::process::Stdio;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::GpuSample;

/// Reads NVIDIA GPU stats by spawning `nvidia-smi` once with the `-lms`
/// "loop in milliseconds" flag so we get a CSV row per interval forever
/// from a single subprocess. A cold `nvidia-smi` call is 150–300 ms, which
/// would dominate the applet's tick budget if we shelled out per tick.
#[derive(Debug)]
pub struct NvidiaSampler {
    cache: Arc<Mutex<Option<GpuSample>>>,
    // Held to keep the streaming task alive for the sampler's lifetime.
    _task: tokio::task::JoinHandle<()>,
}

impl NvidiaSampler {
    /// Constructs the sampler and starts the streaming subprocess.
    /// Returns `None` if `nvidia-smi` isn't on PATH or fails the probe,
    /// so the rest of the applet treats Nvidia as absent.
    pub async fn new(interval_ms: u64) -> Option<Self> {
        if !nvidia_smi_available().await {
            return None;
        }
        let cache: Arc<Mutex<Option<GpuSample>>> = Arc::new(Mutex::new(None));
        let cache_clone = Arc::clone(&cache);
        let task = tokio::spawn(stream_nvidia_smi(interval_ms, cache_clone));
        Some(Self { cache, _task: task })
    }

    /// Returns the most recent sample read from the nvidia-smi stream, or
    /// `None` before the first row arrives.
    pub fn sample(&self) -> Option<GpuSample> {
        self.cache.lock().ok().and_then(|guard| guard.clone())
    }
}

async fn nvidia_smi_available() -> bool {
    Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader,nounits"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn stream_nvidia_smi(interval_ms: u64, cache: Arc<Mutex<Option<GpuSample>>>) {
    let mut child = match Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,temperature.gpu,memory.used,memory.total,clocks.current.graphics,power.draw,name",
            "--format=csv,noheader,nounits",
            "-lms",
            &interval_ms.to_string(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(error) => {
            eprintln!("sysmonitor: failed to spawn nvidia-smi: {error}");
            return;
        }
    };
    let Some(stdout) = child.stdout.take() else {
        eprintln!("sysmonitor: nvidia-smi has no stdout");
        return;
    };
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await
        && let Some(sample) = parse_csv_row(&line)
        && let Ok(mut guard) = cache.lock()
    {
        *guard = Some(sample);
    }
}

/// Parses one CSV row produced by `nvidia-smi --query-gpu=...
/// --format=csv,noheader,nounits`. The column order is fixed by the query
/// in `stream_nvidia_smi`; any drift between them will be caught by the
/// parser test below.
///
/// Returns `None` on any malformed row (wrong column count, unparseable
/// number, `[N/A]` placeholder). The streaming loop just drops that row
/// and waits for the next one.
fn parse_csv_row(row: &str) -> Option<GpuSample> {
    let cols: Vec<&str> = row.split(',').map(str::trim).collect();
    if cols.len() != 7 {
        return None;
    }
    let parse_f = |s: &str| -> Option<f64> {
        if s == "[N/A]" || s.is_empty() {
            None
        } else {
            s.parse().ok()
        }
    };
    let parse_u = |s: &str| -> Option<u64> {
        if s == "[N/A]" || s.is_empty() {
            None
        } else {
            s.parse().ok()
        }
    };

    let util_pct = parse_f(cols[0]);
    let temp_c = parse_f(cols[1]);
    let mem_used_mib = parse_u(cols[2]);
    let mem_total_mib = parse_u(cols[3]);
    let freq_mhz = parse_f(cols[4]);
    let power_w = parse_f(cols[5]);
    let name = if cols[6].is_empty() {
        None
    } else {
        Some(cols[6].to_owned())
    };

    let mem_used_bytes = mem_used_mib.map(|m| m * 1024 * 1024);
    let mem_total_bytes = mem_total_mib.map(|m| m * 1024 * 1024);
    let mem_util = match (mem_used_mib, mem_total_mib) {
        (Some(u), Some(t)) if t > 0 => Some(u as f64 / t as f64),
        _ => None,
    };

    Some(GpuSample {
        name,
        util: util_pct.map(|p| p / 100.0),
        temp_c,
        mem_used_bytes,
        mem_total_bytes,
        mem_util,
        freq_mhz,
        power_w,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Healthy row from a 4090 — sanity-checks the column order is
    /// what we ask nvidia-smi for. The query string in
    /// `stream_nvidia_smi` and the indexing in `parse_csv_row` must stay
    /// in lockstep; this test pins both.
    #[test]
    fn parses_typical_csv_row() {
        let row = "45, 67, 8192, 24576, 1800, 285.50, NVIDIA GeForce RTX 4090";
        let sample = parse_csv_row(row).expect("row parses");
        assert!((sample.util.unwrap() - 0.45).abs() < 1e-12);
        assert_eq!(sample.temp_c, Some(67.0));
        assert_eq!(sample.mem_used_bytes, Some(8192 * 1024 * 1024));
        assert_eq!(sample.mem_total_bytes, Some(24576 * 1024 * 1024));
        assert!((sample.mem_util.unwrap() - (8192.0 / 24576.0)).abs() < 1e-12);
        assert_eq!(sample.freq_mhz, Some(1800.0));
        assert_eq!(sample.power_w, Some(285.50));
        assert_eq!(sample.name.as_deref(), Some("NVIDIA GeForce RTX 4090"));
    }

    /// nvidia-smi reports `[N/A]` for unsupported fields on older
    /// cards or in VMs without NVML access. The parser must accept
    /// the row and surface those as `None` so the indicator hides the
    /// affected tokens instead of pretending power is 0W.
    #[test]
    fn tolerates_na_columns() {
        let row = "[N/A], 50, 1024, 4096, [N/A], [N/A], GeForce GT 730";
        let sample = parse_csv_row(row).expect("row parses despite N/A");
        assert!(sample.util.is_none());
        assert_eq!(sample.temp_c, Some(50.0));
        assert!(sample.freq_mhz.is_none());
        assert!(sample.power_w.is_none());
        assert_eq!(sample.name.as_deref(), Some("GeForce GT 730"));
    }

    /// Wrong column count → reject rather than mis-align fields. Defense
    /// against future nvidia-smi changes that drop or add columns.
    #[test]
    fn rejects_row_with_wrong_column_count() {
        // 5 columns instead of 7.
        assert!(parse_csv_row("1, 2, 3, 4, 5").is_none());
    }
}
