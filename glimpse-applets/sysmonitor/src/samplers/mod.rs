//! Per-tick samplers reading `/proc` and `/sys`. Each sub-module owns one
//! domain (CPU, memory, disk, net, temp) and exposes a `*Sampler` plus a
//! `*Sample`. The `Samplers` bundle drives them together on a shared
//! cadence and returns a `Sample` envelope that's cheap to clone through
//! the SDK's state channel.
//!
//! Lazy-sampled domains (disk, net, temp) are queried only when the
//! relevant indicator/config key is configured — `Samplers::tick` takes a
//! `RequestedSensors` describing what to read. No point statvfs-ing every
//! mountpoint if the user only configured one.

use std::collections::HashMap;
use std::path::PathBuf;

mod amdgpu;
mod cpu;
mod disk;
mod mem;
mod net;
mod nvidia;
mod procs;
mod temp;

pub use amdgpu::AmdgpuSampler;
pub use cpu::{CpuSample, CpuSampler};
pub use disk::{DiskSample, DiskSampler};
pub use mem::{MemSample, MemSampler};
pub use net::{NetSample, NetSampler};
pub use nvidia::NvidiaSampler;
pub use procs::{ProcSampler, ProcTopN, ProcessSample};
pub use temp::{TempSample, TempSampler};

/// Vendor-neutral GPU snapshot. amdgpu fills it from sysfs, nvidia from
/// the `nvidia-smi -lms` stream. Each field is `Option<_>` so the
/// renderer can hide tokens whose underlying file/probe is absent on
/// this hardware — never report a silent 0°C / 0W / 0 MHz.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GpuSample {
    pub name: Option<String>,
    /// `0.0..=1.0`. amdgpu source: `gpu_busy_percent`. nvidia source:
    /// `utilization.gpu`.
    pub util: Option<f64>,
    pub temp_c: Option<f64>,
    pub mem_used_bytes: Option<u64>,
    pub mem_total_bytes: Option<u64>,
    /// `used / total` if both are known.
    pub mem_util: Option<f64>,
    pub freq_mhz: Option<f64>,
    pub power_w: Option<f64>,
}

/// Selectors for the on-demand samplers. Built once per tick from the
/// active `Config::indicators` so the sampler only reads what the panel
/// will actually display.
#[derive(Debug, Clone, Default)]
pub struct RequestedSensors {
    pub mountpoints: Vec<PathBuf>,
    pub interfaces: Vec<String>,
    pub temp_sensors: Vec<String>,
    pub amdgpu: bool,
    pub nvidia: bool,
    /// How many rows to keep in the top-CPU and top-RAM lists. Zero skips
    /// the `/proc` walk entirely — useful when no top-process tile is
    /// configured to avoid iterating ~hundreds of /proc entries per tick.
    pub top_processes_count: usize,
}

#[derive(Debug, Default)]
pub struct Samplers {
    pub cpu: CpuSampler,
    pub mem: MemSampler,
    pub disk: DiskSampler,
    pub net: NetSampler,
    pub temp: TempSampler,
    pub amdgpu: AmdgpuSampler,
    /// `None` when nvidia-smi isn't on PATH. Initialised in
    /// `Samplers::with_nvidia` because spawning the nvidia-smi stream is
    /// async — the default constructor stays sync for tests.
    pub nvidia: Option<NvidiaSampler>,
    pub procs: ProcSampler,
}

impl Samplers {
    pub fn new() -> Self {
        Self {
            cpu: CpuSampler::new(),
            mem: MemSampler::new(),
            disk: DiskSampler::new(),
            net: NetSampler::new(),
            temp: TempSampler::new(),
            amdgpu: AmdgpuSampler::new(),
            nvidia: None,
            procs: ProcSampler::new(),
        }
    }

    /// Async constructor that additionally spawns the nvidia-smi streaming
    /// task when the binary is present on PATH. Falls back gracefully on
    /// pure-AMD or no-GPU hosts.
    pub async fn with_nvidia(interval_ms: u64) -> Self {
        let mut s = Self::new();
        s.nvidia = NvidiaSampler::new(interval_ms).await;
        s
    }

    pub fn tick(&mut self, requested: &RequestedSensors) -> Sample {
        let cpu = self.cpu.tick();
        let mem = self.mem.tick();
        let disks = requested
            .mountpoints
            .iter()
            .filter_map(|mp| {
                self.disk
                    .sample(mp)
                    .map(|s| (mp.to_string_lossy().into_owned(), s))
            })
            .collect();
        let nets = requested
            .interfaces
            .iter()
            .filter_map(|iface| self.net.sample(iface).map(|s| (iface.clone(), s)))
            .collect();
        let temps = requested
            .temp_sensors
            .iter()
            .filter_map(|spec| self.temp.sample(spec).map(|s| (spec.clone(), s)))
            .collect();
        let amdgpu = if requested.amdgpu && self.amdgpu.is_available() {
            self.amdgpu.sample()
        } else {
            None
        };
        let nvidia = if requested.nvidia {
            self.nvidia.as_ref().and_then(|n| n.sample())
        } else {
            None
        };
        let procs = self.procs.tick(requested.top_processes_count);
        Sample {
            cpu,
            mem: mem.mem,
            swap: mem.swap,
            disks,
            nets,
            temps,
            amdgpu,
            nvidia,
            procs,
        }
    }
}

/// Envelope carried from the sampler task to the applet's update loop.
///
/// Cloned (cheaply — small numeric fields plus a few short hash maps; no
/// allocations on the typical render path) into `State` on every tick so
/// `status()` always reads a coherent snapshot. `PartialEq` is required
/// because the SDK's `Msg::Tick(Sample)` carries this through a channel
/// and the SDK's de-dup layer compares messages.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    pub cpu: CpuSample,
    pub mem: MemSample,
    pub swap: MemSample,
    /// Keyed by the mountpoint string from config (e.g. `"/"`, `"/home"`).
    pub disks: HashMap<String, DiskSample>,
    /// Keyed by interface name (e.g. `"wlan0"`).
    pub nets: HashMap<String, NetSample>,
    /// Keyed by the user's sensor spec (e.g. `"coretemp/Package id 0"`).
    pub temps: HashMap<String, TempSample>,
    pub amdgpu: Option<GpuSample>,
    pub nvidia: Option<GpuSample>,
    pub procs: ProcTopN,
}
