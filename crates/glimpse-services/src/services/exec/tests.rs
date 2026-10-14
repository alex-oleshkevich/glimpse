use std::os::unix::fs::PermissionsExt;

use std::path::PathBuf;

use std::sync::{Arc, Mutex};

use glimpse_dbus::{Buses, notifications::NotificationsProvider};

use rustix::pipe::{PipeFlags, pipe_with};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use tokio::net::unix::pipe;

use tokio_util::sync::CancellationToken;

use super::*;

use crate::selection::FakeSelection;

use crate::service::{ServiceRuntime, ServiceSender};

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

fn placement() -> Placement {
    Placement {
        output: Some("DP-1".to_owned()),
        position: Edge::Top,
        orientation: Orientation::Horizontal,
        zone: Zone::Left,
        size: 32,
    }
}

fn script(source: &str) -> (tempfile::TempDir, Entry, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("applet.sh");
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

struct TestConfig {
    config: Config,
    state_dir: tempfile::TempDir,
}

impl std::ops::Deref for TestConfig {
    type Target = Config;

    fn deref(&self) -> &Config {
        &self.config
    }
}

impl std::ops::DerefMut for TestConfig {
    fn deref_mut(&mut self) -> &mut Config {
        &mut self.config
    }
}

fn configured() -> TestConfig {
    let state_dir = tempfile::tempdir().expect("state dir");
    let config = Config {
        state_base: Some(state_dir.path().to_path_buf()),
        bus_proxy: std::path::PathBuf::from(super::PROXY_PROGRAM),
        applets: BTreeMap::from([(
            "test".to_owned(),
            glimpse_config::ExecAppletConfig {
                applet: "me.example.Test".to_owned(),
                options: Map::from_iter([("color".to_owned(), Value::from("blue"))]),
                env: BTreeMap::new(),
                sandbox: glimpse_config::AppletSandbox {
                    read: vec![std::env::temp_dir().display().to_string()],
                    write: vec![std::env::temp_dir().display().to_string()],
                    ..Default::default()
                },
            },
        )]),
    };
    TestConfig { config, state_dir }
}

fn confined(entry: &Entry) -> TestConfig {
    let mut config = configured();
    let path = entry
        .path
        .parent()
        .expect("script parent")
        .display()
        .to_string();
    let sandbox = &mut config.applets.get_mut("test").expect("test applet").sandbox;
    sandbox.read = vec![path.clone()];
    sandbox.write = vec![path];
    config
}

fn landlock_available() -> bool {
    if sandbox::abi() < 10 {
        eprintln!("skipped: Landlock ABI < 10");
        false
    } else {
        true
    }
}

fn running(
    catalog: FakeCatalog,
) -> (
    ExecHandle,
    ServiceSender<Exec>,
    CancellationToken,
    tokio::task::JoinHandle<()>,
    FakeSelection,
) {
    running_with(configured(), Arc::new(catalog))
}

fn running_with(
    config: TestConfig,
    catalog: Arc<dyn Catalog>,
) -> (
    ExecHandle,
    ServiceSender<Exec>,
    CancellationToken,
    tokio::task::JoinHandle<()>,
    FakeSelection,
) {
    let TestConfig { config, state_dir } = config;
    let cancel = CancellationToken::new();
    let (mut runtime, handle) = ServiceRuntime::<Exec>::new(
        config,
        Buses::unavailable("no bus in tests"),
        cancel.clone(),
    );
    let sender = runtime.sender();
    let selection = FakeSelection::default();
    let deps = Dependencies {
        catalog,
        selection: Arc::new(selection.clone()),
        notifications: NotificationsProvider::unavailable("no bus in tests").handle(),
        mpris: ServiceRuntime::<crate::services::mpris::Mpris>::new(
            crate::services::mpris::Config::from(&glimpse_config::Config::default()),
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        )
        .1,
        compositor: ServiceRuntime::<crate::services::compositor::Compositor>::new(
            crate::NoConfig,
            Buses::unavailable("no bus in tests"),
            cancel.clone(),
        )
        .1,
    };
    let task = tokio::spawn(async move {
        let _state_dir = state_dir;
        let _ = runtime.run(deps).await;
    });
    (handle, sender, cancel, task, selection)
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
    tokio::time::timeout(Duration::from_secs(2), async {
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

async fn next_generation(handle: &ExecHandle, slot: u64, generation: u64) -> SlotState {
    tokio::time::timeout(Duration::from_secs(4), async {
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

fn attached_entry() -> Entry {
    Entry {
        argv: Vec::new(),
        cwd: None,
        name: "Attached applet".to_owned(),
        icon: Some("test-symbolic".to_owned()),
        path: PathBuf::new(),
        exec: String::new(),
    }
}

fn attached_peer() -> (Pipes, pipe::Sender, BufReader<pipe::Receiver>) {
    let (from_applet, peer_writer) = pipe_with(PipeFlags::CLOEXEC).expect("applet pipe");
    let (peer_reader, to_applet) = pipe_with(PipeFlags::CLOEXEC).expect("host pipe");
    (
        Pipes {
            to_applet,
            from_applet,
        },
        pipe::Sender::from_owned_fd(peer_writer).expect("peer writer"),
        BufReader::new(pipe::Receiver::from_owned_fd(peer_reader).expect("peer reader")),
    )
}

async fn offer_and_attach(
    handle: &ExecHandle,
    slot: u64,
    applet: &str,
    options: Map<String, Value>,
    pipes: Pipes,
) {
    handle
        .offer(
            applet,
            Offer {
                output: None,
                position: None,
                entry: attached_entry(),
                options,
                sandbox: glimpse_config::AppletSandbox::default(),
                pipes,
            },
        )
        .await
        .expect("offer");
    handle
        .attach(slot, applet, placement())
        .await
        .expect("attach offer");
}

#[tokio::test]
#[ignore = "runs for 61 seconds"]
async fn child_survives_the_blocking_pool_retirement_window() {
    let (_dir, entry, output) = script(
        "#!/usr/bin/env python3\nimport os, sys, time\nopen(sys.argv[1] + '.pid', 'w').write(str(os.getpid()))\nprint('{\"t\":\"hello\",\"v\":1}', flush=True)\nsys.stdin.readline()\ntime.sleep(70)\n",
    );
    let (handle, _sender, cancel, task, _selection) =
        running(FakeCatalog(Arc::new(Mutex::new(Ok(entry)))));
    handle.attach(1, "test", placement()).await.expect("attach");
    status(&handle, 1, |s| matches!(s, Status::Running)).await;
    let pid: u32 = tokio::fs::read_to_string(format!("{}.pid", output.display()))
        .await
        .expect("pid")
        .trim()
        .parse()
        .expect("numeric pid");
    tokio::time::sleep(Duration::from_secs(61)).await;
    assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
    assert!(matches!(
        handle.snapshot().slots.get(&1).map(|s| &s.status),
        Some(Status::Running)
    ));
    handle.detach(1).await.expect("detach");
    cancel.cancel();
    let _ = task.await;
}

mod attached;
mod confinement;
mod lifecycle;
mod protocol;
