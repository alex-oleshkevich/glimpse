use std::collections::BTreeMap;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use glimpse_dbus::systemd1::{Systemd1ManagerProxy, Systemd1ServiceProxy, Systemd1UnitProxy};
use zbus::zvariant::{Fd, Value};

pub const NEEDS_SYSTEMD: &str = "needs the systemd user manager";

pub const EXIT_UNAVAILABLE: i32 = 69;

pub struct UnitSpec {
    pub name: String,
    pub description: String,
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub restart: bool,
    pub stderr: Option<OwnedFd>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitState {
    Restarting { restarts: u32 },
    Failed { result: String },
    Gone,
}

pub trait Launcher: Send + Sync + 'static {
    fn start(&self, spec: UnitSpec) -> BoxFuture<'static, Result<(), String>>;
    fn stop(&self, name: &str) -> BoxFuture<'static, ()>;
    fn kill(&self, name: &str) -> BoxFuture<'static, ()>;
    fn reset_failed(&self, name: &str) -> BoxFuture<'static, ()>;
    fn watch(&self, name: &str) -> BoxStream<'static, UnitState>;
    fn sweep(&self, prefix: &str) -> BoxFuture<'static, ()>;
}

pub fn escape(component: &str) -> String {
    component.replace('-', "\\x2d")
}

pub fn unit_name(panel: &str, applet: &str, slot: u64, n: u64) -> String {
    let pid = std::process::id();
    format!(
        "app-glimpse-{}-{}-{slot}-{pid}-{n}.service",
        escape(panel),
        escape(applet)
    )
}

pub fn sweep_prefix(panel: &str) -> String {
    format!("app-glimpse-{}-", escape(panel))
}

pub fn panel_pid(name: &str) -> Option<u32> {
    let body = name
        .strip_prefix("app-glimpse-")?
        .strip_suffix(".service")?;
    let parts: Vec<&str> = body.split('-').collect();
    let [panel, applet, slot, pid, n] = parts[..] else {
        return None;
    };
    let decimal = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if panel.is_empty() || applet.is_empty() || !decimal(slot) || !decimal(pid) || !decimal(n) {
        return None;
    }
    pid.parse().ok()
}

pub fn is_orphan_unit(name: &str, own_pid: u32, alive: impl Fn(u32) -> bool) -> bool {
    panel_pid(name).is_some_and(|pid| pid != own_pid && !alive(pid))
}

fn pid_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

fn set_env(env: &mut Vec<(String, String)>, name: String, value: String) {
    if value.contains('\0') {
        return;
    }
    if let Some(entry) = env.iter_mut().find(|(key, _)| *key == name) {
        entry.1 = value;
    } else {
        env.push((name, value));
    }
}

pub fn build_env(
    panel_env: Vec<(String, String)>,
    socket: &Path,
    config_env: &BTreeMap<String, String>,
) -> Vec<(String, String)> {
    let mut env = Vec::new();
    for (name, value) in panel_env {
        if glimpse_config::exec_env_name_allowed(&name) {
            set_env(&mut env, name, value);
        }
    }
    set_env(
        &mut env,
        "GLIMPSE_APPLET_SOCKET".to_owned(),
        socket.display().to_string(),
    );
    for (name, value) in config_env {
        if glimpse_config::exec_env_name_allowed(name) {
            set_env(&mut env, name.clone(), value.clone());
        }
    }
    env
}

pub fn panel_env() -> Vec<(String, String)> {
    std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect()
}

pub fn resolve_program(program: &str, path_env: Option<&str>) -> Option<String> {
    if program.contains('/') {
        return Some(program.to_owned());
    }
    let path_env = path_env?;
    for dir in path_env.split(':').filter(|dir| !dir.is_empty()) {
        let candidate = Path::new(dir).join(program);
        if is_executable(&candidate) {
            return Some(candidate.display().to_string());
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[allow(clippy::too_many_arguments)]
pub fn build_spec(
    name: String,
    description: String,
    mut argv: Vec<String>,
    cwd: Option<PathBuf>,
    panel_environment: Vec<(String, String)>,
    socket: &Path,
    config_env: &BTreeMap<String, String>,
    restart: bool,
    stderr: Option<OwnedFd>,
) -> Result<UnitSpec, String> {
    let Some(program) = argv.first().cloned() else {
        return Err("empty argv".to_owned());
    };
    let env = build_env(panel_environment, socket, config_env);
    let path_env = env
        .iter()
        .find(|(key, _)| key == "PATH")
        .map(|(_, value)| value.as_str());
    let resolved =
        resolve_program(&program, path_env).ok_or_else(|| format!("{program} not found"))?;
    argv[0] = resolved;
    Ok(UnitSpec {
        name,
        description,
        argv,
        cwd,
        env,
        restart,
        stderr,
    })
}

pub fn unit_properties(spec: &UnitSpec) -> Result<Vec<(&'static str, Value<'_>)>, String> {
    let Some(program) = spec.argv.first().cloned() else {
        return Err("empty argv".to_owned());
    };
    let exec = vec![(program, spec.argv.clone(), false)];
    let environment: Vec<String> = spec
        .env
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    let mut properties: Vec<(&'static str, Value<'_>)> = vec![
        ("Description", Value::from(spec.description.as_str())),
        ("Type", Value::from("exec")),
        ("ExecStart", Value::from(exec)),
        ("Environment", Value::from(environment)),
        ("Slice", Value::from("app.slice")),
        ("CollectMode", Value::from("inactive-or-failed")),
        (
            "Restart",
            Value::from(if spec.restart { "always" } else { "no" }),
        ),
        (
            "RestartPreventExitStatus",
            Value::from((vec![EXIT_UNAVAILABLE], Vec::<i32>::new())),
        ),
        ("RestartUSec", Value::from(1_000_000u64)),
        ("RestartSteps", Value::from(6u32)),
        ("RestartMaxDelayUSec", Value::from(60_000_000u64)),
        ("StartLimitIntervalUSec", Value::from(0u64)),
        ("MemoryMax", Value::from(256u64 << 20)),
    ];
    if let Some(dir) = &spec.cwd {
        let cwd = dir
            .to_str()
            .ok_or_else(|| "working directory is not utf-8".to_owned())?;
        properties.push(("WorkingDirectory", Value::from(cwd)));
    }
    if let Some(stderr) = &spec.stderr {
        properties.push(("StandardErrorFileDescriptor", Value::from(Fd::from(stderr))));
    }
    Ok(properties)
}

pub struct SystemdLauncher {
    inner: std::sync::Arc<Inner>,
}

struct Inner {
    connection: Option<zbus::Connection>,
    manager: tokio::sync::OnceCell<Systemd1ManagerProxy<'static>>,
    warned: AtomicBool,
}

impl SystemdLauncher {
    pub fn new(connection: Option<zbus::Connection>) -> Self {
        Self {
            inner: std::sync::Arc::new(Inner {
                connection,
                manager: tokio::sync::OnceCell::new(),
                warned: AtomicBool::new(false),
            }),
        }
    }
}

impl Inner {
    fn unavailable(&self) -> String {
        if !self.warned.swap(true, Ordering::Relaxed) {
            tracing::warn!("exec applets need the systemd user manager on the session bus");
        }
        NEEDS_SYSTEMD.to_owned()
    }

    async fn manager(&self) -> Result<Systemd1ManagerProxy<'static>, String> {
        self.manager
            .get_or_try_init(|| async {
                let Some(connection) = &self.connection else {
                    return Err(self.unavailable());
                };
                let proxy = Systemd1ManagerProxy::new(connection)
                    .await
                    .map_err(|_| self.unavailable())?;
                proxy.subscribe().await.map_err(|_| self.unavailable())?;
                Ok(proxy)
            })
            .await
            .cloned()
    }
}

impl Launcher for SystemdLauncher {
    fn start(&self, spec: UnitSpec) -> BoxFuture<'static, Result<(), String>> {
        let inner = std::sync::Arc::clone(&self.inner);
        Box::pin(async move {
            let manager = inner.manager().await?;
            let properties = unit_properties(&spec)?;
            manager
                .start_transient_unit(&spec.name, "fail", &properties, &[])
                .await
                .map(drop)
                .map_err(|error| error.to_string())
        })
    }

    fn stop(&self, name: &str) -> BoxFuture<'static, ()> {
        let inner = std::sync::Arc::clone(&self.inner);
        let name = name.to_owned();
        Box::pin(async move {
            if let Ok(manager) = inner.manager().await {
                let _ = manager.stop_unit(&name, "replace").await;
            }
        })
    }

    fn kill(&self, name: &str) -> BoxFuture<'static, ()> {
        let inner = std::sync::Arc::clone(&self.inner);
        let name = name.to_owned();
        Box::pin(async move {
            if let Ok(manager) = inner.manager().await {
                let _ = manager.kill_unit(&name, "all", 9).await;
            }
        })
    }

    fn reset_failed(&self, name: &str) -> BoxFuture<'static, ()> {
        let inner = std::sync::Arc::clone(&self.inner);
        let name = name.to_owned();
        Box::pin(async move {
            if let Ok(manager) = inner.manager().await {
                let _ = manager.reset_failed_unit(&name).await;
            }
        })
    }

    fn watch(&self, name: &str) -> BoxStream<'static, UnitState> {
        let connection = self.inner.connection.clone();
        let name = name.to_owned();
        Box::pin(futures_util::stream::unfold(
            Watch::new(name, connection),
            |mut watch| async move { watch.next().await.map(|state| (state, watch)) },
        ))
    }

    fn sweep(&self, prefix: &str) -> BoxFuture<'static, ()> {
        let inner = std::sync::Arc::clone(&self.inner);
        let prefix = prefix.to_owned();
        Box::pin(async move {
            let Ok(manager) = inner.manager().await else {
                return;
            };
            let own = format!("{prefix}*");
            let Ok(units) = manager
                .list_units_by_patterns(&[], &[&own, "app-glimpse-*.service"])
                .await
            else {
                return;
            };
            let own_pid = std::process::id();
            for unit in units {
                if unit.0.starts_with(&prefix) || is_orphan_unit(&unit.0, own_pid, pid_alive) {
                    let _ = manager.stop_unit(&unit.0, "replace").await;
                }
            }
        })
    }
}

enum Change {
    Removed,
    Sub(String),
    Active(String),
}

struct Watch {
    name: String,
    connection: Option<zbus::Connection>,
    unit: Option<Systemd1UnitProxy<'static>>,
    service: Option<Systemd1ServiceProxy<'static>>,
    changes: Option<BoxStream<'static, Change>>,
    restarting: bool,
    done: bool,
}

impl Watch {
    fn new(name: String, connection: Option<zbus::Connection>) -> Self {
        Self {
            name,
            connection,
            unit: None,
            service: None,
            changes: None,
            restarting: false,
            done: false,
        }
    }

    async fn subscribe(&mut self) -> Option<()> {
        use futures_util::{StreamExt, future, stream};
        let connection = self.connection.clone()?;
        let manager = Systemd1ManagerProxy::new(&connection).await.ok()?;
        let _ = manager.subscribe().await;
        let path = manager.get_unit(&self.name).await.ok()?;
        let unit = Systemd1UnitProxy::builder(&connection)
            .path(path.clone())
            .ok()?
            .build()
            .await
            .ok()?;
        let service = Systemd1ServiceProxy::builder(&connection)
            .path(path)
            .ok()?
            .build()
            .await
            .ok()?;
        let name = self.name.clone();
        let removed = manager
            .receive_unit_removed()
            .await
            .ok()?
            .filter_map(move |signal| {
                let hit = signal.args().is_ok_and(|args| args.id == name);
                future::ready(hit.then_some(Change::Removed))
            });
        let subs = unit
            .receive_sub_state_changed()
            .await
            .then(|change| async move { Change::Sub(change.get().await.unwrap_or_default()) });
        let actives = unit
            .receive_active_state_changed()
            .await
            .then(|change| async move { Change::Active(change.get().await.unwrap_or_default()) });
        self.changes = Some(stream::select(removed, stream::select(subs, actives)).boxed());
        self.unit = Some(unit);
        self.service = Some(service);
        Some(())
    }

    async fn next(&mut self) -> Option<UnitState> {
        use futures_util::StreamExt;
        if self.done {
            return None;
        }
        if self.changes.is_none() {
            self.subscribe().await?;
        }
        loop {
            match self.changes.as_mut()?.next().await? {
                Change::Removed => {
                    self.done = true;
                    return Some(UnitState::Gone);
                }
                Change::Sub(value) => {
                    let restarting = value.contains("auto-restart");
                    let entered = restarting && !self.restarting;
                    self.restarting = restarting;
                    if entered {
                        let restarts = self
                            .service
                            .as_ref()?
                            .n_restarts()
                            .await
                            .unwrap_or_default();
                        return Some(UnitState::Restarting { restarts });
                    }
                }
                Change::Active(value) if value == "failed" => {
                    let sub = self.unit.as_ref()?.sub_state().await.unwrap_or_default();
                    if !sub.contains("auto-restart") {
                        let result = self
                            .service
                            .as_ref()?
                            .result()
                            .await
                            .unwrap_or_else(|_| "failed".to_owned());
                        return Some(UnitState::Failed { result });
                    }
                }
                Change::Active(_) => {}
            }
        }
    }
}

#[cfg(test)]
pub use process::ProcessLauncher;

#[cfg(test)]
mod process;

#[cfg(test)]
mod tests;
