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
