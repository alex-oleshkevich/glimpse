use std::collections::HashMap;
use std::fs;
use std::time::Instant;

/// Per-interface throughput, computed as a delta between two reads of
/// `/sys/class/net/<iface>/statistics/{rx,tx}_bytes`. First tick yields
/// zero rate (no prior reading) — the second tick onward is accurate.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NetSample {
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub rx_total_bytes: u64,
    pub tx_total_bytes: u64,
}

#[derive(Debug, Clone)]
struct PrevReading {
    rx: u64,
    tx: u64,
    at: Instant,
}

#[derive(Debug, Default)]
pub struct NetSampler {
    prev: HashMap<String, PrevReading>,
}

impl NetSampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sample a single interface. Returns `None` if the sysfs path is
    /// absent (interface unplugged / renamed since the last config load).
    pub fn sample(&mut self, interface: &str) -> Option<NetSample> {
        let rx = read_counter(interface, "rx_bytes")?;
        let tx = read_counter(interface, "tx_bytes")?;
        let now = Instant::now();
        let sample = match self.prev.get(interface) {
            Some(prev) => sample_from_delta(prev, rx, tx, now),
            None => NetSample {
                rx_total_bytes: rx,
                tx_total_bytes: tx,
                ..Default::default()
            },
        };
        self.prev
            .insert(interface.to_owned(), PrevReading { rx, tx, at: now });
        Some(sample)
    }
}

fn sample_from_delta(prev: &PrevReading, rx: u64, tx: u64, now: Instant) -> NetSample {
    let dt = now.saturating_duration_since(prev.at).as_secs_f64();
    let rate = |delta: u64| -> f64 {
        if dt <= 0.0 {
            0.0
        } else {
            (delta as f64) / dt
        }
    };
    NetSample {
        // saturating_sub so a counter rollover (rare, but possible on
        // 32-bit `statistics` fields on older kernels) clamps to zero
        // instead of producing a huge spike.
        rx_bytes_per_sec: rate(rx.saturating_sub(prev.rx)),
        tx_bytes_per_sec: rate(tx.saturating_sub(prev.tx)),
        rx_total_bytes: rx,
        tx_total_bytes: tx,
    }
}

fn read_counter(interface: &str, file: &str) -> Option<u64> {
    let path = format!("/sys/class/net/{interface}/statistics/{file}");
    let raw = fs::read_to_string(path).ok()?;
    raw.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// 1 MiB delta over 1 second → 1 MiB/s. Verifies the time-axis
    /// arithmetic isn't being computed in raw nanoseconds (which would
    /// give nonsense).
    #[test]
    fn rate_is_bytes_divided_by_seconds() {
        let now = Instant::now();
        let prev = PrevReading {
            rx: 0,
            tx: 0,
            at: now - Duration::from_secs(1),
        };
        let sample = sample_from_delta(&prev, 1_048_576, 524_288, now);
        assert!(
            (sample.rx_bytes_per_sec - 1_048_576.0).abs() < 1.0,
            "got {}",
            sample.rx_bytes_per_sec
        );
        assert!((sample.tx_bytes_per_sec - 524_288.0).abs() < 1.0);
    }

    /// Counter wrap (current < prev) must clamp to zero — never report
    /// a negative or astronomical rate. Saturating_sub is the guardrail.
    #[test]
    fn wraparound_clamps_to_zero() {
        let now = Instant::now();
        let prev = PrevReading {
            rx: 1_000_000,
            tx: 1_000_000,
            at: now - Duration::from_secs(1),
        };
        // New reading lower than prev (counter wrapped or reset).
        let sample = sample_from_delta(&prev, 100, 200, now);
        assert_eq!(sample.rx_bytes_per_sec, 0.0);
        assert_eq!(sample.tx_bytes_per_sec, 0.0);
        // Totals still reflect the current (post-wrap) counter value.
        assert_eq!(sample.rx_total_bytes, 100);
    }

    /// Two reads at the same instant should not divide by zero.
    #[test]
    fn zero_elapsed_time_yields_zero_rate() {
        let now = Instant::now();
        let prev = PrevReading {
            rx: 0,
            tx: 0,
            at: now,
        };
        let sample = sample_from_delta(&prev, 1000, 1000, now);
        assert!(sample.rx_bytes_per_sec.is_finite());
        assert_eq!(sample.rx_bytes_per_sec, 0.0);
    }
}
