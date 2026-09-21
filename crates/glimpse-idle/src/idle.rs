use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use glimpse_config::{Idle, IdleListener};
use tokio::process::Command as TokioCommand;
use tokio::sync::{Mutex as AsyncMutex, mpsc, watch};
use tokio_util::sync::CancellationToken;

const COMMAND_QUEUE_SIZE: usize = 16;

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
    command_tx: mpsc::Sender<Event>,
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

    pub fn try_send(&self, event: Event) -> Result<(), mpsc::error::TrySendError<Event>> {
        self.command_tx.try_send(event)
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
            let _guard = lock.lock().await;
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
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use glimpse_config::{IdleProfile as Profile, IdleProfiles as Profiles};
    use tokio::sync::{Notify, oneshot};

    use super::*;

    #[derive(Clone, Default)]
    struct Recorded(Arc<Mutex<Vec<String>>>);

    struct RecordingRunner {
        commands: Recorded,
    }

    impl CommandRunner for RecordingRunner {
        fn run<'a>(&'a self, command: &'a str) -> BoxFuture<'a> {
            let commands = self.commands.clone();
            let command = command.to_owned();
            Box::pin(async move {
                commands.0.lock().unwrap().push(command);
            })
        }
    }

    struct BlockingRunner {
        started: Mutex<Option<oneshot::Sender<()>>>,
        release: Notify,
    }

    impl CommandRunner for BlockingRunner {
        fn run<'a>(&'a self, _command: &'a str) -> BoxFuture<'a> {
            Box::pin(async move {
                if let Some(started) = self.started.lock().unwrap().take() {
                    let _ = started.send(());
                }
                self.release.notified().await;
            })
        }
    }

    fn test_config() -> Idle {
        Idle {
            enabled: true,
            respect_inhibitors: true,
            profiles: Profiles {
                ac: Profile {
                    listeners: vec![
                        IdleListener {
                            timeout: 10,
                            on_idle: "ac-idle".into(),
                            on_resume: "ac-resume".into(),
                            respect_inhibitors: None,
                        },
                        IdleListener {
                            timeout: 0,
                            on_idle: "disabled".into(),
                            on_resume: String::new(),
                            respect_inhibitors: None,
                        },
                    ],
                },
                battery: Profile {
                    listeners: vec![IdleListener {
                        timeout: 5,
                        on_idle: "battery-idle".into(),
                        on_resume: "battery-resume".into(),
                        respect_inhibitors: None,
                    }],
                },
            },
        }
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    #[tokio::test]
    async fn battery_change_reresolves_listeners_from_battery_profile() {
        let (actor, handle) = Actor::new(
            test_config(),
            false,
            Arc::new(RecordingRunner {
                commands: Recorded::default(),
            }),
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(actor.run(cancel.clone()));

        handle.send(Event::OnBattery(true)).await;
        settle().await;

        let state = handle.snapshot();
        assert_eq!(state.power_source, PowerSource::Battery);
        assert_eq!(state.listeners.len(), 1);
        assert_eq!(state.listeners[0].timeout, 5);

        cancel.cancel();
        let _ = task.await;
    }
    #[tokio::test]
    async fn battery_change_resumes_fired_listener_before_switching_and_clears_fired() {
        let commands = Recorded::default();
        let (actor, handle) = Actor::new(
            test_config(),
            false,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(actor.run(cancel.clone()));

        handle
            .send(Event::ListenerIdle {
                generation: 0,
                id: 0,
            })
            .await;
        settle().await;
        handle.send(Event::OnBattery(true)).await;
        settle().await;

        let state = handle.snapshot();
        assert_eq!(state.power_source, PowerSource::Battery);
        assert!(state.fired_listeners.is_empty());
        assert_eq!(
            *commands.0.lock().unwrap(),
            vec!["ac-idle".to_string(), "ac-resume".to_string()]
        );

        cancel.cancel();
        let _ = task.await;
    }
    #[tokio::test]
    async fn in_flight_command_does_not_block_next_transition() {
        let (started_tx, started_rx) = oneshot::channel();
        let runner = Arc::new(BlockingRunner {
            started: Mutex::new(Some(started_tx)),
            release: Notify::new(),
        });
        let (actor, handle) = Actor::new(test_config(), false, runner.clone());
        let cancel = CancellationToken::new();
        let task = tokio::spawn(actor.run(cancel.clone()));

        handle
            .send(Event::ListenerIdle {
                generation: 0,
                id: 0,
            })
            .await;
        started_rx
            .await
            .expect("the blocking command should signal it started");

        handle
            .send(Event::ListenerResume {
                generation: 0,
                id: 0,
            })
            .await;
        settle().await;

        assert!(handle.snapshot().fired_listeners.is_empty());

        runner.release.notify_waiters();
        settle().await;
        cancel.cancel();
        let _ = task.await;
    }
    #[tokio::test]
    async fn config_change_resumes_fired_listener_before_replacing_policy() {
        let commands = Recorded::default();
        let (actor, handle) = Actor::new(
            test_config(),
            false,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(actor.run(cancel.clone()));

        handle
            .send(Event::ListenerIdle {
                generation: 0,
                id: 0,
            })
            .await;
        settle().await;

        let mut next_config = test_config();
        next_config.profiles.ac.listeners[0].on_resume = "next-resume".into();
        handle.send(Event::ApplyConfig(next_config)).await;
        settle().await;

        assert_eq!(
            *commands.0.lock().unwrap(),
            vec!["ac-idle".to_string(), "ac-resume".to_string()]
        );
        assert!(handle.snapshot().fired_listeners.is_empty());

        cancel.cancel();
        let _ = task.await;
    }
    #[tokio::test]
    async fn disabled_config_reports_disabled_health_and_runs_nothing() {
        let commands = Recorded::default();
        let mut config = test_config();
        config.enabled = false;
        let (actor, handle) = Actor::new(
            config,
            false,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(actor.run(cancel.clone()));

        let state = handle.snapshot();
        assert!(
            state.listeners.is_empty(),
            "`enabled = false` resolves to no listeners at all"
        );

        handle
            .send(Event::ListenerIdle {
                generation: 0,
                id: 0,
            })
            .await;
        settle().await;
        assert!(commands.0.lock().unwrap().is_empty());
        assert!(handle.snapshot().fired_listeners.is_empty());

        cancel.cancel();
        let _ = task.await;
    }
    #[tokio::test]
    async fn stale_generation_listener_event_is_ignored() {
        let commands = Recorded::default();
        let (actor, handle) = Actor::new(
            test_config(),
            false,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(actor.run(cancel.clone()));

        handle.send(Event::OnBattery(true)).await;
        settle().await;
        assert_eq!(handle.snapshot().generation, 1);

        handle
            .send(Event::ListenerIdle {
                generation: 0,
                id: 0,
            })
            .await;
        settle().await;

        assert!(handle.snapshot().fired_listeners.is_empty());
        assert!(commands.0.lock().unwrap().is_empty());

        cancel.cancel();
        let _ = task.await;
    }
    #[test]
    fn resolve_listeners_filters_disabled_and_resolves_inhibitor_override() {
        let mut config = test_config();
        config.profiles.ac.listeners.push(IdleListener {
            timeout: 20,
            on_idle: "explicit-false".into(),
            on_resume: String::new(),
            respect_inhibitors: Some(false),
        });

        let listeners = resolve_listeners(&config, PowerSource::Ac);

        assert_eq!(listeners.len(), 2, "the timeout == 0 listener is dropped");
        assert!(
            listeners[0].respect_inhibitors,
            "no override falls back to the table default"
        );
        assert!(
            !listeners[1].respect_inhibitors,
            "an explicit override wins over the table default"
        );
    }
    #[tokio::test]
    async fn editing_the_inactive_profile_does_not_disturb_the_active_policy() {
        let commands = Recorded::default();
        let (actor, handle) = Actor::new(
            test_config(),
            false,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(actor.run(cancel.clone()));

        handle
            .send(Event::ListenerIdle {
                generation: 0,
                id: 0,
            })
            .await;
        settle().await;
        assert_eq!(handle.snapshot().generation, 0);
        assert_eq!(handle.snapshot().fired_listeners, vec![0]);

        let mut next_config = test_config();
        next_config.profiles.battery.listeners[0].timeout = 999;
        handle.send(Event::ApplyConfig(next_config)).await;
        settle().await;

        let state = handle.snapshot();
        assert_eq!(
            state.generation, 0,
            "an unrelated profile edit must not bump the generation"
        );
        assert_eq!(
            state.fired_listeners,
            vec![0],
            "the fired listener must not be disturbed"
        );
        assert_eq!(
            *commands.0.lock().unwrap(),
            vec!["ac-idle".to_string()],
            "no on_resume should run for an unrelated edit"
        );

        cancel.cancel();
        let _ = task.await;
    }
    #[tokio::test]
    async fn respecting_listener_does_not_fire_while_an_idle_target_inhibitor_exists() {
        let commands = Recorded::default();
        let (actor, handle) = Actor::new(
            test_config(),
            false,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(actor.run(cancel.clone()));

        handle.send(Event::RegistryChanged(true)).await;
        settle().await;
        handle
            .send(Event::ListenerIdle {
                generation: 0,
                id: 0,
            })
            .await;
        settle().await;

        assert!(handle.snapshot().fired_listeners.is_empty());
        assert!(commands.0.lock().unwrap().is_empty());

        cancel.cancel();
        let _ = task.await;
    }
    #[tokio::test]
    async fn releasing_the_last_idle_target_fires_a_suppressed_listener_immediately() {
        let commands = Recorded::default();
        let (actor, handle) = Actor::new(
            test_config(),
            false,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(actor.run(cancel.clone()));

        handle.send(Event::RegistryChanged(true)).await;
        settle().await;
        handle
            .send(Event::ListenerIdle {
                generation: 0,
                id: 0,
            })
            .await;
        settle().await;
        assert!(handle.snapshot().fired_listeners.is_empty());

        handle.send(Event::RegistryChanged(false)).await;
        settle().await;

        assert_eq!(handle.snapshot().fired_listeners, vec![0]);
        assert_eq!(*commands.0.lock().unwrap(), vec!["ac-idle".to_string()]);

        cancel.cancel();
        let _ = task.await;
    }
    #[tokio::test]
    async fn a_fired_listener_stays_fired_when_a_new_inhibitor_appears() {
        let commands = Recorded::default();
        let (actor, handle) = Actor::new(
            test_config(),
            false,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(actor.run(cancel.clone()));

        handle
            .send(Event::ListenerIdle {
                generation: 0,
                id: 0,
            })
            .await;
        settle().await;
        assert_eq!(handle.snapshot().fired_listeners, vec![0]);

        handle.send(Event::RegistryChanged(true)).await;
        settle().await;

        assert_eq!(
            handle.snapshot().fired_listeners,
            vec![0],
            "fired stays sticky across a new inhibitor appearing"
        );
        assert_eq!(
            *commands.0.lock().unwrap(),
            vec!["ac-idle".to_string()],
            "no on_resume must run just because an inhibitor appeared"
        );

        cancel.cancel();
        let _ = task.await;
    }
    #[tokio::test]
    async fn non_respecting_listener_fires_regardless_of_registry_state() {
        let commands = Recorded::default();
        let mut config = test_config();
        config.profiles.ac.listeners[0].respect_inhibitors = Some(false);
        let (actor, handle) = Actor::new(
            config,
            false,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(actor.run(cancel.clone()));

        handle.send(Event::RegistryChanged(true)).await;
        settle().await;
        handle
            .send(Event::ListenerIdle {
                generation: 0,
                id: 0,
            })
            .await;
        settle().await;

        assert_eq!(handle.snapshot().fired_listeners, vec![0]);
        assert_eq!(*commands.0.lock().unwrap(), vec!["ac-idle".to_string()]);

        cancel.cancel();
        let _ = task.await;
    }
    #[tokio::test]
    async fn a_stale_generation_suppressed_entry_cannot_fire_under_a_new_policy() {
        let commands = Recorded::default();
        let (actor, handle) = Actor::new(
            test_config(),
            false,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task = tokio::spawn(actor.run(cancel.clone()));

        handle.send(Event::RegistryChanged(true)).await;
        settle().await;
        handle
            .send(Event::ListenerIdle {
                generation: 0,
                id: 0,
            })
            .await;
        settle().await;
        assert!(handle.snapshot().fired_listeners.is_empty());

        handle.send(Event::OnBattery(true)).await;
        settle().await;
        assert_eq!(handle.snapshot().generation, 1);

        handle.send(Event::RegistryChanged(false)).await;
        settle().await;

        assert!(
            handle.snapshot().fired_listeners.is_empty(),
            "a suppression from the old generation must not fire the new policy's same-index listener"
        );
        assert!(commands.0.lock().unwrap().is_empty());

        cancel.cancel();
        let _ = task.await;
    }
}
