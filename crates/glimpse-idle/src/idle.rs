use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use glimpse_config::{Idle, IdleListener};
use tokio::process::Command as TokioCommand;
use tokio::sync::{Mutex as AsyncMutex, mpsc, watch};
use tokio_util::sync::CancellationToken;

const COMMAND_QUEUE_SIZE: usize = 16;
const COMMAND_WAIT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    Ac,
    Battery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveListener {
    pub id: usize,
    pub timeout: u64,
    pub on_idle: String,
    pub on_resume: String,
    pub respect_inhibitors: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub enabled: bool,
    pub power_source: PowerSource,
    pub listeners: Vec<ActiveListener>,
    pub fired_listeners: Vec<usize>,
    pub generation: u64,
}

#[derive(Debug, Clone)]
pub enum Event {
    ApplyConfig(Idle),
    OnBattery(bool),
    ListenerIdle { generation: u64, id: usize },
    ListenerResume { generation: u64, id: usize },
    RegistryChanged(bool),
    BackendReset,
}

type BoxFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

pub trait CommandRunner: Send + Sync {
    fn run<'a>(&'a self, command: &'a str) -> BoxFuture<'a>;
}

pub struct ShellRunner;

impl CommandRunner for ShellRunner {
    fn run<'a>(&'a self, command: &'a str) -> BoxFuture<'a> {
        Box::pin(async move {
            match TokioCommand::new("/bin/sh")
                .arg("-c")
                .arg(command)
                .status()
                .await
            {
                Ok(status) if status.success() => {
                    tracing::info!(command, "idle listener command completed");
                }
                Ok(status) => {
                    tracing::warn!(
                        command,
                        code = status.code(),
                        "idle listener command failed"
                    );
                }
                Err(error) => {
                    tracing::warn!(command, %error, "idle listener command failed to start");
                }
            }
        })
    }
}

#[derive(Clone)]
pub struct Handle {
    pub(crate) command_tx: mpsc::Sender<Event>,
    state_rx: watch::Receiver<State>,
}

impl Handle {
    pub fn subscribe(&self) -> watch::Receiver<State> {
        self.state_rx.clone()
    }

    pub fn snapshot(&self) -> State {
        self.state_rx.borrow().clone()
    }

    pub async fn send(&self, event: Event) {
        let _ = self.command_tx.send(event).await;
    }
}

pub struct Actor {
    runner: Arc<dyn CommandRunner>,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<Event>,
    config: Idle,
    power_source: PowerSource,
    fired: HashSet<usize>,
    suppressed: HashSet<usize>,
    any_idle_target: bool,
    command_locks: HashMap<usize, Arc<AsyncMutex<()>>>,
}

impl Actor {
    pub fn new(config: Idle, on_battery: bool, runner: Arc<dyn CommandRunner>) -> (Self, Handle) {
        let power_source = power_source_for(on_battery);
        let state = state_for_config(&config, power_source, 0);
        let command_locks = command_locks_for(&state.listeners);
        let (state_tx, state_rx) = watch::channel(state);
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                runner,
                state_tx,
                command_rx,
                config,
                power_source,
                fired: HashSet::new(),
                suppressed: HashSet::new(),
                any_idle_target: false,
                command_locks,
            },
            Handle {
                command_tx,
                state_rx,
            },
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        tracing::debug!("idle actor started");
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                event = self.command_rx.recv() => match event {
                    Some(Event::ApplyConfig(config)) => self.apply_config(config),
                    Some(Event::OnBattery(on_battery)) => self.set_on_battery(on_battery),
                    Some(Event::ListenerIdle { generation, id }) => self.listener_idle(generation, id),
                    Some(Event::ListenerResume { generation, id }) => self.listener_resume(generation, id),
                    Some(Event::RegistryChanged(value)) => self.set_any_idle_target(value),
                    Some(Event::BackendReset) => self.backend_reset(),
                    None => break,
                }
            }
        }
        tracing::debug!("idle actor stopped");
    }

    fn apply_config(&mut self, config: Idle) {
        if self.config == config {
            return;
        }
        self.config = config;
        self.replace_policy();
    }

    fn set_on_battery(&mut self, on_battery: bool) {
        let next = power_source_for(on_battery);
        if self.power_source == next {
            return;
        }
        self.power_source = next;
        self.replace_policy();
    }

    fn replace_policy(&mut self) {
        let current = self.state_tx.borrow().clone();
        let candidate_listeners = effective_listeners(&self.config, self.power_source);
        if current.enabled == self.config.enabled
            && current.power_source == self.power_source
            && current.listeners == candidate_listeners
        {
            return;
        }

        self.resume_fired_before_replace();
        self.fired.clear();
        self.suppressed.clear();

        let state = state_for_config(&self.config, self.power_source, current.generation + 1);
        self.sync_command_locks(&state.listeners);
        self.state_tx.send_replace(state);
    }

    fn listener_idle(&mut self, generation: u64, id: usize) {
        if generation != self.state_tx.borrow().generation {
            return;
        }
        let Some(listener) = self.active_listener(id) else {
            return;
        };
        if self.fired.contains(&id) {
            return;
        }
        if listener.respect_inhibitors && self.any_idle_target {
            self.suppressed.insert(id);
            return;
        }
        self.fire(id, listener.on_idle);
    }

    fn listener_resume(&mut self, generation: u64, id: usize) {
        if generation != self.state_tx.borrow().generation {
            return;
        }
        let Some(listener) = self.active_listener(id) else {
            return;
        };
        self.suppressed.remove(&id);
        if !self.fired.remove(&id) {
            return;
        }
        self.publish_fired();
        self.spawn_command(id, listener.on_resume);
    }

    fn fire(&mut self, id: usize, on_idle: String) {
        self.fired.insert(id);
        self.publish_fired();
        self.spawn_command(id, on_idle);
    }

    fn set_any_idle_target(&mut self, value: bool) {
        self.any_idle_target = value;
        if value {
            return;
        }
        for id in sorted(&std::mem::take(&mut self.suppressed)) {
            let Some(listener) = self.active_listener(id) else {
                continue;
            };
            self.fire(id, listener.on_idle);
        }
    }

    fn backend_reset(&mut self) {
        self.resume_fired_before_replace();
        self.fired.clear();
        self.suppressed.clear();
        self.publish_fired();
    }

    fn resume_fired_before_replace(&mut self) {
        for id in sorted(&self.fired) {
            let Some(listener) = self.active_listener(id) else {
                continue;
            };
            self.spawn_command(id, listener.on_resume);
        }
    }

    fn spawn_command(&self, id: usize, command: String) {
        let command = command.trim().to_owned();
        if command.is_empty() {
            return;
        }
        let Some(lock) = self.command_locks.get(&id).cloned() else {
            return;
        };
        let runner = self.runner.clone();
        tokio::spawn(async move {
            let _guard = match tokio::time::timeout(COMMAND_WAIT, lock.lock()).await {
                Ok(guard) => Some(guard),
                Err(_) => {
                    tracing::warn!(
                        listener = id,
                        command,
                        "previous listener command still running, running queued command anyway"
                    );
                    None
                }
            };
            runner.run(&command).await;
        });
    }

    fn active_listener(&self, id: usize) -> Option<ActiveListener> {
        self.state_tx
            .borrow()
            .listeners
            .iter()
            .find(|listener| listener.id == id)
            .cloned()
    }

    fn publish_fired(&self) {
        let fired = sorted(&self.fired);
        self.state_tx.send_if_modified(|state| {
            if state.fired_listeners == fired {
                false
            } else {
                state.fired_listeners = fired;
                true
            }
        });
    }

    fn sync_command_locks(&mut self, listeners: &[ActiveListener]) {
        self.command_locks
            .retain(|id, _| listeners.iter().any(|listener| listener.id == *id));
        for listener in listeners {
            self.command_locks
                .entry(listener.id)
                .or_insert_with(|| Arc::new(AsyncMutex::new(())));
        }
    }
}

fn power_source_for(on_battery: bool) -> PowerSource {
    if on_battery {
        PowerSource::Battery
    } else {
        PowerSource::Ac
    }
}

fn state_for_config(config: &Idle, power_source: PowerSource, generation: u64) -> State {
    State {
        enabled: config.enabled,
        power_source,
        listeners: effective_listeners(config, power_source),
        fired_listeners: vec![],
        generation,
    }
}

fn effective_listeners(config: &Idle, power_source: PowerSource) -> Vec<ActiveListener> {
    if config.enabled {
        resolve_listeners(config, power_source)
    } else {
        vec![]
    }
}

fn resolve_listeners(config: &Idle, power_source: PowerSource) -> Vec<ActiveListener> {
    let listeners = match power_source {
        PowerSource::Ac => &config.profiles.ac.listeners,
        PowerSource::Battery => &config.profiles.battery.listeners,
    };

    listeners
        .iter()
        .enumerate()
        .filter(|(_, listener)| listener.timeout > 0)
        .map(|(id, listener)| active_listener(id, listener, config.respect_inhibitors))
        .collect()
}

fn active_listener(
    id: usize,
    listener: &IdleListener,
    default_respect_inhibitors: bool,
) -> ActiveListener {
    ActiveListener {
        id,
        timeout: listener.timeout,
        on_idle: listener.on_idle.clone(),
        on_resume: listener.on_resume.clone(),
        respect_inhibitors: listener
            .respect_inhibitors
            .unwrap_or(default_respect_inhibitors),
    }
}

fn command_locks_for(listeners: &[ActiveListener]) -> HashMap<usize, Arc<AsyncMutex<()>>> {
    listeners
        .iter()
        .map(|listener| (listener.id, Arc::new(AsyncMutex::new(()))))
        .collect()
}

fn sorted(ids: &HashSet<usize>) -> Vec<usize> {
    let mut ids = ids.iter().copied().collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

#[cfg(test)]
mod tests;
