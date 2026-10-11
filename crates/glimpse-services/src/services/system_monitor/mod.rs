mod sample;

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use futures_util::stream;

use crate::{
    context::Ctx,
    publisher::Publisher,
    service::{Input, Service},
    subscription::Sub,
};

use sample::{Discovery, Sampled};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cpu {
    pub percent: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadAverage {
    pub one: f32,
    pub five: f32,
    pub fifteen: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub used: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskUsage {
    pub path: String,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetworkRate {
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuMemoryKind {
    Vram,
    Gtt,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gpu {
    pub usage_percent: Option<f32>,
    pub memory: Usage,
    pub memory_kind: GpuMemoryKind,
    pub temp_c: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemMonitorState {
    pub cpu: Option<Cpu>,
    pub cpu_temperature: Option<f32>,
    pub load_average: Option<LoadAverage>,
    pub memory: Option<Usage>,
    pub swap: Option<Usage>,
    pub disks: Vec<DiskUsage>,
    pub network: Option<NetworkRate>,
    pub uptime: Option<Duration>,
    pub gpu: Option<Gpu>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub enabled: bool,
    pub poll_interval: Duration,
    pub disk_paths: Vec<PathBuf>,
    pub gpu: bool,
}

impl From<&glimpse_config::Config> for Config {
    fn from(document: &glimpse_config::Config) -> Self {
        let enabled = glimpse_config::placed_kinds(document)
            .any(|kind| matches!(kind, glimpse_config::AppletKind::SystemMonitor(_)));
        Self {
            enabled,
            poll_interval: Duration::from_secs(document.system_monitor.poll_interval),
            disk_paths: document
                .system_monitor
                .disk_paths
                .iter()
                .map(PathBuf::from)
                .collect(),
            gpu: document.system_monitor.gpu,
        }
    }
}

pub type Command = Infallible;

pub enum Event {
    Sampled(Sampled),
    Disks(Vec<DiskUsage>),
    Discovered(Discovery),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Watch {
    Sample(u64),
    Disks(u64, Vec<PathBuf>),
    Discover(u64),
}

struct Previous {
    at: Instant,
    cpu_total: u64,
    cpu_idle: u64,
    network: HashMap<String, (u64, u64)>,
}

pub struct SystemMonitor {
    state: Publisher<SystemMonitorState>,
    config: Config,
    previous: Option<Previous>,
    discovery: Arc<Mutex<Discovery>>,
    discover_generation: u64,
}

pub type SystemMonitorHandle = crate::ServiceEndpoint<SystemMonitor>;

impl Service for SystemMonitor {
    const NAME: &'static str = "system-monitor";
    type Config = Config;
    type State = SystemMonitorState;
    type Command = Command;
    type Event = Event;
    type Dependencies = ();
    type SubKey = Watch;

    fn initial_state(_: &Self::Config) -> Self::State {
        Self::State::default()
    }

    fn subscriptions(&self) -> Vec<Sub<Self>> {
        if !self.config.enabled {
            return Vec::new();
        }

        let period = self.config.poll_interval;
        let seconds = period.as_secs();
        let gpu_enabled = self.config.gpu;

        vec![
            Sub::interval(Watch::Sample(seconds), period, {
                let discovery = Arc::clone(&self.discovery);
                move |_ctx| {
                    let discovery = Arc::clone(&discovery);
                    async move {
                        let raw = tokio::task::spawn_blocking(move || {
                            let discovery = discovery
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .clone();
                            sample::sample_proc_and_sysfs(&discovery)
                        })
                        .await
                        .unwrap_or_default();
                        Event::Sampled(raw)
                    }
                }
            }),
            Sub::interval(
                Watch::Disks(seconds, self.config.disk_paths.clone()),
                period,
                {
                    let paths = self.config.disk_paths.clone();
                    move |_ctx| {
                        let paths = paths.clone();
                        async move {
                            let disks =
                                tokio::task::spawn_blocking(move || sample::sample_disks(&paths))
                                    .await
                                    .unwrap_or_default();
                            Event::Disks(disks)
                        }
                    }
                },
            ),
            Sub::stream(
                Watch::Discover(self.discover_generation),
                move |_ctx| async move {
                    stream::once(async move {
                        let discovery =
                            tokio::task::spawn_blocking(move || sample::discover(gpu_enabled))
                                .await
                                .unwrap_or_default();
                        Event::Discovered(discovery)
                    })
                },
            ),
        ]
    }

    async fn start(ctx: &Ctx<Self>, config: Self::Config, (): Self::Dependencies) -> Self {
        Self {
            state: ctx.publisher(),
            config,
            previous: None,
            discovery: Arc::new(Mutex::new(Discovery::default())),
            discover_generation: 0,
        }
    }

    async fn handle(&mut self, ctx: &Ctx<Self>, input: Input<Self>) {
        match input {
            Input::Command(never) => match never {},
            Input::Config(next) => {
                let gpu_flip = self.config.enabled && next.enabled && self.config.gpu != next.gpu;
                let disabling = self.config.enabled && !next.enabled;
                self.config = next;
                self.previous = None;
                if gpu_flip {
                    self.discover_generation += 1;
                }
                if disabling {
                    *self
                        .discovery
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = Discovery::default();
                    self.state.set(SystemMonitorState::default());
                }
            }
            Input::Event(Event::Sampled(raw)) => {
                if !self.config.enabled {
                    return;
                }
                ctx.running();
                self.apply_sample(raw);
            }
            Input::Event(Event::Disks(disks)) => {
                if !self.config.enabled {
                    return;
                }
                ctx.running();
                self.state.update(|state| state.disks = disks);
            }
            Input::Event(Event::Discovered(discovery)) => {
                if !self.config.enabled {
                    return;
                }
                *self
                    .discovery
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = discovery;
            }
        }
    }
}

impl SystemMonitor {
    fn apply_sample(&mut self, raw: Sampled) {
        let now = Instant::now();

        let cpu = raw.cpu_total.zip(raw.cpu_idle).and_then(|(total, idle)| {
            self.previous.as_ref().and_then(|previous| {
                let total_delta = total.checked_sub(previous.cpu_total)?;
                let idle_delta = idle.checked_sub(previous.cpu_idle)?;
                (total_delta > 0).then(|| {
                    let busy = total_delta.saturating_sub(idle_delta.min(total_delta));
                    Cpu {
                        percent: (busy as f32 / total_delta as f32) * 100.0,
                    }
                })
            })
        });

        let network_map: HashMap<String, (u64, u64)> = raw
            .network
            .iter()
            .cloned()
            .map(|(name, rx, tx)| (name, (rx, tx)))
            .collect();
        let network = self.previous.as_ref().map(|previous| {
            let elapsed = now.duration_since(previous.at).as_secs_f64();
            let (mut rx_total, mut tx_total) = (0u64, 0u64);
            for (name, (rx, tx)) in &network_map {
                if let Some((previous_rx, previous_tx)) = previous.network.get(name) {
                    rx_total += rx.checked_sub(*previous_rx).unwrap_or(0);
                    tx_total += tx.checked_sub(*previous_tx).unwrap_or(0);
                }
            }
            if elapsed > 0.0 {
                NetworkRate {
                    rx_bytes_per_sec: rx_total as f64 / elapsed,
                    tx_bytes_per_sec: tx_total as f64 / elapsed,
                }
            } else {
                NetworkRate {
                    rx_bytes_per_sec: 0.0,
                    tx_bytes_per_sec: 0.0,
                }
            }
        });

        self.previous = raw
            .cpu_total
            .zip(raw.cpu_idle)
            .map(|(total, idle)| Previous {
                at: now,
                cpu_total: total,
                cpu_idle: idle,
                network: network_map,
            });

        self.state.update(|state| {
            state.cpu = cpu;
            state.cpu_temperature = raw.cpu_temperature;
            state.load_average = raw.load_average;
            state.memory = raw.memory;
            state.swap = raw.swap;
            state.network = network;
            state.uptime = raw.uptime;
            state.gpu = raw.gpu;
        });
    }
}

#[cfg(test)]
mod tests;
