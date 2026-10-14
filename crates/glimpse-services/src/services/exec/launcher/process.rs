use std::collections::HashMap;
use std::os::unix::process::ExitStatusExt;
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use futures_util::stream;
use futures_util::stream::BoxStream;
use tokio::process::Command;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::{Launcher, UnitSpec, UnitState};

struct Unit {
    stop: CancellationToken,
    pid: Arc<Mutex<Option<u32>>>,
    state: watch::Sender<Option<UnitState>>,
    restarts: Arc<AtomicU32>,
}

pub struct ProcessLauncher {
    units: Arc<Mutex<HashMap<String, Arc<Unit>>>>,
    kills: Arc<AtomicU32>,
    starts: Arc<Mutex<Vec<String>>>,
}

impl Default for ProcessLauncher {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessLauncher {
    pub fn new() -> Self {
        Self {
            units: Arc::new(Mutex::new(HashMap::new())),
            kills: Arc::new(AtomicU32::new(0)),
            starts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn kill_count(&self) -> u32 {
        self.kills.load(Ordering::Relaxed)
    }

    pub fn started(&self) -> Vec<String> {
        self.starts
            .lock()
            .map(|starts| starts.clone())
            .unwrap_or_default()
    }

    fn unit(&self, name: &str) -> Option<Arc<Unit>> {
        self.units.lock().ok()?.get(name).cloned()
    }

    fn kill_pid(unit: &Unit) {
        if let Some(pid) = unit.pid.lock().ok().and_then(|pid| *pid)
            && let Some(pid) = rustix::process::Pid::from_raw(pid as i32)
        {
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
        }
    }
}

fn exit_reason(status: std::process::ExitStatus) -> String {
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exited with status {code}"),
        (None, Some(signal)) => format!("killed by signal {signal}"),
        (None, None) => status.to_string(),
    }
}

impl Launcher for ProcessLauncher {
    fn start(&self, spec: UnitSpec) -> BoxFuture<'static, Result<(), String>> {
        let units = Arc::clone(&self.units);
        if let Ok(mut starts) = self.starts.lock() {
            starts.push(spec.name.clone());
        }
        Box::pin(async move {
            let (tx, _rx) = watch::channel(None);
            let unit = Arc::new(Unit {
                stop: CancellationToken::new(),
                pid: Arc::new(Mutex::new(None)),
                state: tx,
                restarts: Arc::new(AtomicU32::new(0)),
            });
            if let Ok(mut map) = units.lock() {
                map.insert(spec.name.clone(), Arc::clone(&unit));
            }
            let UnitSpec {
                argv,
                cwd,
                env,
                restart,
                stderr,
                ..
            } = spec;
            let Some(program) = argv.first().cloned() else {
                return Err("empty argv".to_owned());
            };
            let stop = unit.stop.clone();
            let pid_slot = Arc::clone(&unit.pid);
            let state = unit.state.clone();
            let restarts = Arc::clone(&unit.restarts);
            tokio::spawn(async move {
                let mut stderr = stderr;
                loop {
                    let mut command = Command::new(&program);
                    command
                        .args(argv.iter().skip(1))
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .kill_on_drop(true);
                    if let Some(cwd) = &cwd {
                        command.current_dir(cwd);
                    }
                    command.env_clear();
                    command.envs(env.iter().cloned());
                    match stderr.take() {
                        Some(fd) => {
                            let file = std::fs::File::from(fd);
                            match file.try_clone() {
                                Ok(clone) => {
                                    stderr = Some(std::os::fd::OwnedFd::from(file));
                                    command.stderr(Stdio::from(clone));
                                }
                                Err(_) => {
                                    command.stderr(Stdio::from(file));
                                }
                            }
                        }
                        None => {
                            command.stderr(Stdio::null());
                        }
                    }
                    let mut child = match command.spawn() {
                        Ok(child) => child,
                        Err(error) => {
                            let _ = state.send(Some(UnitState::Failed {
                                result: format!("cannot start applet: {error}"),
                            }));
                            return;
                        }
                    };
                    if let Ok(mut slot) = pid_slot.lock() {
                        *slot = child.id();
                    }
                    let status = tokio::select! {
                        biased;
                        _ = stop.cancelled() => {
                            let _ = child.start_kill();
                            let _ = child.wait().await;
                            let _ = state.send(Some(UnitState::Gone));
                            return;
                        }
                        status = child.wait() => status,
                    };
                    if let Ok(mut slot) = pid_slot.lock() {
                        *slot = None;
                    }
                    if stop.is_cancelled() {
                        let _ = state.send(Some(UnitState::Gone));
                        return;
                    }
                    match status {
                        Ok(status) if status.success() => {
                            let _ = state.send(Some(UnitState::Gone));
                            return;
                        }
                        Ok(status) if restart => {
                            let count = restarts.fetch_add(1, Ordering::Relaxed) + 1;
                            let _ = state.send(Some(UnitState::Restarting { restarts: count }));
                            let _ = status;
                        }
                        Ok(status) => {
                            let _ = state.send(Some(UnitState::Failed {
                                result: exit_reason(status),
                            }));
                            return;
                        }
                        Err(error) => {
                            let _ = state.send(Some(UnitState::Failed {
                                result: error.to_string(),
                            }));
                            return;
                        }
                    }
                }
            });
            Ok(())
        })
    }

    fn stop(&self, name: &str) -> BoxFuture<'static, ()> {
        let unit = self.unit(name);
        Box::pin(async move {
            if let Some(unit) = unit {
                unit.stop.cancel();
                Self::kill_pid(&unit);
                let _ = unit.state.send(Some(UnitState::Gone));
            }
        })
    }

    fn kill(&self, name: &str) -> BoxFuture<'static, ()> {
        let unit = self.unit(name);
        let kills = Arc::clone(&self.kills);
        Box::pin(async move {
            if let Some(unit) = unit {
                kills.fetch_add(1, Ordering::Relaxed);
                Self::kill_pid(&unit);
            }
        })
    }

    fn reset_failed(&self, name: &str) -> BoxFuture<'static, ()> {
        let unit = self.unit(name);
        Box::pin(async move {
            if let Some(unit) = unit {
                unit.restarts.store(0, Ordering::Relaxed);
            }
        })
    }

    fn watch(&self, name: &str) -> BoxStream<'static, UnitState> {
        let Some(unit) = self.unit(name) else {
            return Box::pin(stream::once(async { UnitState::Gone }));
        };
        let receiver = unit.state.subscribe();
        Box::pin(stream::unfold(
            (receiver, false),
            |(mut receiver, done)| async move {
                if done {
                    return None;
                }
                loop {
                    if receiver.changed().await.is_err() {
                        return None;
                    }
                    let current = receiver.borrow_and_update().clone();
                    if let Some(state) = current {
                        let done = matches!(state, UnitState::Gone);
                        return Some((state, (receiver, done)));
                    }
                }
            },
        ))
    }

    fn sweep(&self, prefix: &str) -> BoxFuture<'static, ()> {
        let units: Vec<Arc<Unit>> = self
            .units
            .lock()
            .map(|map| {
                map.iter()
                    .filter(|(name, _)| name.starts_with(prefix))
                    .map(|(_, unit)| Arc::clone(unit))
                    .collect()
            })
            .unwrap_or_default();
        Box::pin(async move {
            for unit in units {
                unit.stop.cancel();
                Self::kill_pid(&unit);
                let _ = unit.state.send(Some(UnitState::Gone));
            }
        })
    }
}
