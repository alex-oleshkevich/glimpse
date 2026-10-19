use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use glimpse_dbus::Buses;
use tokio_util::sync::CancellationToken;

use super::launcher::{Launcher, NEEDS_SYSTEMD, ProcessLauncher, UnitSpec, UnitState};
use super::*;
use crate::service::ServiceRuntime;

#[derive(Clone)]
struct FakeCatalog(Arc<Mutex<Result<Entry, String>>>);

impl Catalog for FakeCatalog {
    fn resolve(&self, _id: &str) -> Result<Entry, String> {
        self.0.lock().map_err(|error| error.to_string())?.clone()
    }
}

struct PairCatalog {
    bad: Entry,
    good: Entry,
}

impl Catalog for PairCatalog {
    fn resolve(&self, id: &str) -> Result<Entry, String> {
        match id {
            "me.example.Test" => Ok(self.bad.clone()),
            "me.example.Other" => Ok(self.good.clone()),
            _ => Err("unknown applet".to_owned()),
        }
    }
}

struct FailingLauncher;

impl Launcher for FailingLauncher {
    fn start(&self, _spec: UnitSpec) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Err(NEEDS_SYSTEMD.to_owned()) })
    }
    fn stop(&self, _name: &str) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
    fn kill(&self, _name: &str) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
    fn reset_failed(&self, _name: &str) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
    fn watch(&self, _name: &str) -> BoxStream<'static, UnitState> {
        Box::pin(futures_util::stream::empty())
    }
    fn sweep(&self, _prefix: &str) -> BoxFuture<'static, ()> {
        Box::pin(async {})
    }
}

#[derive(Default)]
struct ControlledLauncher {
    process: Arc<ProcessLauncher>,
    unavailable: std::sync::atomic::AtomicBool,
    reset_calls: std::sync::atomic::AtomicU32,
    sweep_started: Arc<tokio::sync::Notify>,
    sweep_release: Arc<tokio::sync::Notify>,
}

impl Launcher for ControlledLauncher {
    fn start(&self, spec: UnitSpec) -> BoxFuture<'static, Result<(), String>> {
        if self.unavailable.load(std::sync::atomic::Ordering::Relaxed) {
            Box::pin(async { Err(NEEDS_SYSTEMD.to_owned()) })
        } else {
            self.process.start(spec)
        }
    }

    fn stop(&self, name: &str) -> BoxFuture<'static, ()> {
        self.process.stop(name)
    }

    fn kill(&self, name: &str) -> BoxFuture<'static, ()> {
        self.process.kill(name)
    }

    fn reset_failed(&self, name: &str) -> BoxFuture<'static, ()> {
        self.reset_calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.process.reset_failed(name)
    }

    fn watch(&self, name: &str) -> BoxStream<'static, UnitState> {
        self.process.watch(name)
    }

    fn sweep(&self, prefix: &str) -> BoxFuture<'static, ()> {
        let process = Arc::clone(&self.process);
        let prefix = prefix.to_owned();
        let started = Arc::clone(&self.sweep_started);
        let release = Arc::clone(&self.sweep_release);
        Box::pin(async move {
            started.notify_one();
            release.notified().await;
            process.sweep(&prefix).await;
        })
    }
}

fn placement() -> Placement {
    Placement {
        output: Some("DP-1".to_owned()),
        position: Edge::Top,
        orientation: Orientation::Horizontal,
        zone: Zone::Left,
        size: 32,
    }
}

const PREAMBLE: &str = "#!/usr/bin/env python3\nimport os, socket, sys\n_sock = socket.socket(socket.AF_UNIX)\n_sock.connect(os.environ[\"GLIMPSE_APPLET_SOCKET\"])\nr = _sock.makefile(\"r\", encoding=\"utf-8\")\ndef send(line):\n    _sock.sendall((line + \"\\n\").encode())\n";

fn script(body: &str) -> (tempfile::TempDir, Entry, PathBuf) {
    raw_script(&format!("{PREAMBLE}{body}"))
}

fn raw_script(source: &str) -> (tempfile::TempDir, Entry, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("applet.py");
    std::fs::write(&path, source).expect("script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let output = dir.path().join("messages");
    let entry = Entry {
        argv: vec![path.display().to_string(), output.display().to_string()],
        cwd: None,
        name: "Test applet".to_owned(),
        icon: None,
        path: path.clone(),
        exec: path.display().to_string(),
    };
    (dir, entry, output)
}

const LOOP: &str =
    "send('{\"t\":\"hello\",\"v\":1}')\nfor line in r:\n    open(sys.argv[1], 'a').write(line)\n";

struct TestConfig {
    config: Config,
    _state_dir: tempfile::TempDir,
    _runtime_dir: tempfile::TempDir,
}

fn configured() -> TestConfig {
    let state_dir = tempfile::tempdir().expect("state dir");
    let runtime_dir = tempfile::tempdir().expect("runtime dir");
    let config = Config {
        state_base: Some(state_dir.path().to_path_buf()),
        runtime_base: Some(runtime_dir.path().to_path_buf()),
        panel: "me.aresa.TestPanel".to_owned(),
        applets: BTreeMap::from([(
            "test".to_owned(),
            glimpse_config::ExecAppletConfig {
                applet: "me.example.Test".to_owned(),
                options: Map::from_iter([("color".to_owned(), Value::from("blue"))]),
                env: BTreeMap::new(),
            },
        )]),
    };
    TestConfig {
        config,
        _state_dir: state_dir,
        _runtime_dir: runtime_dir,
    }
}

type Running = (
    ExecHandle,
    tokio::sync::mpsc::Sender<Input<Exec>>,
    CancellationToken,
    tokio::task::JoinHandle<()>,
    Arc<ProcessLauncher>,
);

fn running(catalog: FakeCatalog) -> (Running, Config) {
    let test = configured();
    let snapshot = test.config.clone();
    (running_with(test, Arc::new(catalog), None), snapshot)
}

fn running_with(
    config: TestConfig,
    catalog: Arc<dyn Catalog>,
    launcher: Option<Arc<dyn Launcher>>,
) -> Running {
    let TestConfig {
        config,
        _state_dir,
        _runtime_dir,
    } = config;
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Exec>::new(
        config,
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    let sender = runtime.sender();
    let process = Arc::new(ProcessLauncher::new());
    let deps = Dependencies {
        catalog,
        launcher: launcher.unwrap_or_else(|| Arc::clone(&process) as Arc<dyn Launcher>),
    };
    let task = tokio::spawn(async move {
        let _keep = (_state_dir, _runtime_dir);
        runtime.run(deps).await;
    });
    (handle, sender, cancel, task, process)
}

async fn status(handle: &ExecHandle, slot: u64, predicate: impl Fn(&Status) -> bool) -> SlotState {
    let mut receiver = handle.subscribe();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Some(state) = receiver
                .borrow()
                .slots
                .get(&slot)
                .filter(|state| predicate(&state.status))
            {
                return state.clone();
            }
            receiver.changed().await.expect("service state");
        }
    })
    .await
    .expect("status changed")
}

async fn file_contains(path: &PathBuf, needle: &str) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(content) = tokio::fs::read_to_string(path).await
                && content.contains(needle)
            {
                return content;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("message written")
}

async fn file_lines(path: &PathBuf, count: usize) -> Vec<String> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(content) = tokio::fs::read_to_string(path).await
                && content.lines().count() >= count
            {
                return content.lines().map(str::to_owned).collect();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("lines written")
}

async fn next_generation(handle: &ExecHandle, slot: u64, generation: u64) -> SlotState {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(state) = handle
                .snapshot()
                .slots
                .get(&slot)
                .filter(|s| matches!(s.status, Status::Running) && s.generation > generation)
            {
                return state.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("next generation")
}

async fn pid_gone(pid: &str) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while std::path::Path::new(&format!("/proc/{}", pid.trim())).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("process gone");
}

fn dev_entry(entry: &Entry) -> Entry {
    Entry {
        name: "Dev applet".to_owned(),
        icon: Some("dev-symbolic".to_owned()),
        ..entry.clone()
    }
}

fn dev_request(entry: Entry) -> DevRequest {
    DevRequest {
        entry,
        output: None,
        position: None,
        options: Map::new(),
        env: BTreeMap::new(),
        stderr: None,
    }
}

#[tokio::test]
async fn restart_command_starts_a_failed_slot() {
    let (_dir, entry, _output) = script(LOOP);
    let catalog = FakeCatalog(Arc::new(Mutex::new(Err("not installed".to_owned()))));
    let ((handle, _sender, cancel, task, _launcher), _config) = running(catalog.clone());
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Failed(_))).await;
    *catalog.0.lock().expect("catalog") = Ok(entry);
    handle.restart(1).await.expect("restart");
    assert_eq!(handle.snapshot().slots[&1].status, Status::Starting);
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

mod dev;
mod lifecycle;
mod protocol;
