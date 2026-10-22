use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsFd;
use std::os::fd::FromRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use glimpse_dbus::{applet_host::APPLET_HOST_PATH, applet_host_name, own_name};
use tokio::sync::mpsc;
use zbus::zvariant::Fd;

const ID: &str = "org.example.DevTest";

struct Bus {
    daemon: Child,
    address: String,
}

struct Manager(mpsc::UnboundedSender<(String, String)>);

#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    fn list_units_by_patterns(
        &self,
        _states: Vec<String>,
        _patterns: Vec<String>,
    ) -> Vec<glimpse_dbus::systemd1::Systemd1UnitEntry> {
        vec![(
            "app-glimpse-panel-org.example.DevTest-1-4242-1.service".to_owned(),
            "Dev Test".to_owned(),
            "loaded".to_owned(),
            "active".to_owned(),
            "running".to_owned(),
            String::new(),
            "/org/freedesktop/systemd1/unit/test".try_into().unwrap(),
            0,
            String::new(),
            "/".try_into().unwrap(),
        )]
    }

    fn restart_unit(&self, name: String, mode: String) -> zbus::zvariant::OwnedObjectPath {
        self.0.send((name, mode)).unwrap();
        "/org/freedesktop/systemd1/job/1".try_into().unwrap()
    }
}

#[tokio::test]
async fn restart_explicitly_restarts_the_systemd_unit() -> Result<()> {
    let bus = Bus::start()?;
    let connection = zbus::connection::Builder::address(bus.address.as_str())?
        .build()
        .await?;
    let (tx, mut events) = mpsc::unbounded_channel();
    connection
        .object_server()
        .at("/org/freedesktop/systemd1", Manager(tx))
        .await?;
    own_name(&connection, "org.freedesktop.systemd1").await?;
    let output = tokio::time::timeout(
        Duration::from_secs(8),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
            .args(["applet", "restart", ID])
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
            .output(),
    )
    .await??;
    ensure!(output.status.success(), "{output:?}");
    assert_eq!(
        events.try_recv()?,
        (
            "app-glimpse-panel-org.example.DevTest-1-4242-1.service".to_owned(),
            "replace".to_owned()
        )
    );
    Ok(())
}

impl Bus {
    fn start() -> Result<Self> {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()?;
        let address = BufReader::new(daemon.stdout.as_mut().context("no bus stdout")?)
            .lines()
            .next()
            .context("no bus address")??;
        Ok(Self { daemon, address })
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

#[derive(Debug, PartialEq)]
enum Event {
    Attach {
        path: String,
        panel: u32,
        output: String,
        zone: String,
        options: String,
        env: HashMap<String, String>,
        handle: String,
    },
    Restart(String),
    Detach(String),
    Options(String, String),
    Tree(String),
}

#[derive(Clone)]
struct Host {
    tx: mpsc::UnboundedSender<Event>,
    attached: Arc<AtomicUsize>,
    refuse_attach: Arc<AtomicBool>,
    fail_restart: Arc<AtomicBool>,
}

fn host_tree() -> String {
    let mut tree = glimpse_services::Tree::default();
    for id in 1..=150 {
        tree = tree
            .apply(vec![glimpse_services::Op::Insert {
                parent: id - 1,
                node: glimpse_services::WireNode {
                    id,
                    kind: if id == 1 { "popover" } else { "box" }.to_owned(),
                    props: serde_json::Map::new(),
                    children: Vec::new(),
                },
                before: None,
            }])
            .unwrap();
    }
    tree.snapshot().to_string()
}

#[zbus::interface(name = "me.aresa.Glimpse.AppletHost1")]
impl Host {
    #[allow(clippy::too_many_arguments)]
    async fn attach(
        &self,
        desktop_file: &str,
        panel: u32,
        output: &str,
        zone: &str,
        options: &str,
        env: HashMap<String, String>,
        stderr: Fd<'_>,
    ) -> zbus::fdo::Result<String> {
        if self.refuse_attach.load(Ordering::SeqCst) {
            return Err(zbus::fdo::Error::Failed(format!(
                "{ID} is already attached"
            )));
        }
        let mut stderr = fs::File::from(
            stderr
                .as_fd()
                .try_clone_to_owned()
                .map_err(|error| zbus::fdo::Error::IOError(error.to_string()))?,
        );
        let _ = writeln!(stderr, "host-wrote-stderr");
        let handle = format!("dev-{}", self.attached.fetch_add(1, Ordering::SeqCst));
        let _ = self.tx.send(Event::Attach {
            path: desktop_file.to_owned(),
            panel,
            output: output.to_owned(),
            zone: zone.to_owned(),
            options: options.to_owned(),
            env,
            handle: handle.clone(),
        });
        Ok(handle)
    }

    async fn restart(&self, handle: &str) -> zbus::fdo::Result<()> {
        let _ = self.tx.send(Event::Restart(handle.to_owned()));
        if self.fail_restart.swap(false, Ordering::SeqCst) {
            return Err(zbus::fdo::Error::Failed("unknown handle".to_owned()));
        }
        Ok(())
    }

    async fn tree(&self, handle: &str) -> zbus::fdo::Result<String> {
        let _ = self.tx.send(Event::Tree(handle.to_owned()));
        Ok(host_tree())
    }

    async fn detach(&self, handle: &str) -> zbus::fdo::Result<()> {
        let _ = self.tx.send(Event::Detach(handle.to_owned()));
        Ok(())
    }

    async fn set_options(&self, handle: &str, options: &str) -> zbus::fdo::Result<()> {
        let _ = self
            .tx
            .send(Event::Options(handle.to_owned(), options.to_owned()));
        Ok(())
    }
}

struct Setup<'a> {
    config: Option<&'a str>,
    justfile: Option<(&'a str, &'a str)>,
    extra: &'a [&'a str],
    refuse_attach: bool,
    serve: bool,
}

impl Default for Setup<'_> {
    fn default() -> Self {
        Self {
            config: Some(""),
            justfile: None,
            extra: &[],
            refuse_attach: false,
            serve: true,
        }
    }
}

struct Harness {
    _bus: Bus,
    _connection: zbus::Connection,
    root: tempfile::TempDir,
    host: Host,
    events: mpsc::UnboundedReceiver<Event>,
    command: Option<Child>,
    applet: PathBuf,
    desktop: PathBuf,
}

impl Harness {
    async fn dev(config: &str, extra: &[&str]) -> Result<Self> {
        let mut args = vec!["--dev"];
        args.extend_from_slice(extra);
        Self::start(Setup {
            config: Some(config),
            extra: &args,
            ..Setup::default()
        })
        .await
    }

    async fn start(setup: Setup<'_>) -> Result<Self> {
        let bus = Bus::start()?;
        let connection = zbus::connection::Builder::address(bus.address.as_str())?
            .build()
            .await?;
        let (tx, events) = mpsc::unbounded_channel();
        let host = Host {
            tx,
            attached: Arc::new(AtomicUsize::new(0)),
            refuse_attach: Arc::new(AtomicBool::new(setup.refuse_attach)),
            fail_restart: Arc::new(AtomicBool::new(false)),
        };
        if setup.serve {
            connection
                .object_server()
                .at(APPLET_HOST_PATH, host.clone())
                .await?;
            own_name(&connection, &applet_host_name("org.example.TestPanel")).await?;
        }
        let root = tempfile::tempdir()?;
        let applet = root.path().join("applet");
        fs::create_dir(&applet)?;
        let desktop = applet.join(format!("{ID}.desktop"));
        fs::write(
            &desktop,
            "[Desktop Entry]\nType=Application\nName=Dev Test\nExec=/bin/true\nNoDisplay=true\nImplements=me.aresa.Glimpse.Applet1\n",
        )?;
        if let Some(config) = setup.config {
            fs::write(applet.join("devconfig.toml"), config)?;
        }
        if let Some((name, recipe)) = setup.justfile {
            fs::write(applet.join(name), recipe)?;
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_glimpsectl"));
        command
            .args(["applet", "run"])
            .arg(&applet)
            .args(setup.extra)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
            .env("GLIMPSE_PANEL_APP_ID", "org.example.TestPanel")
            .env("XDG_DATA_HOME", root.path().join("data"))
            .stdout(Stdio::piped())
            .stderr(fs::File::create(root.path().join("command.stderr"))?);
        unsafe {
            command.pre_exec(|| {
                let flags = libc::CLOSE_RANGE_CLOEXEC as libc::c_int;
                if libc::close_range(3, libc::c_uint::MAX, flags) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn()?;
        Ok(Self {
            _bus: bus,
            _connection: connection,
            root,
            host,
            events,
            command: Some(child),
            applet,
            desktop,
        })
    }

    async fn event(&mut self) -> Result<Event> {
        tokio::time::timeout(Duration::from_secs(8), self.events.recv())
            .await
            .context("no event within 8 s")?
            .context("event stream closed")
    }

    async fn attach(&mut self) -> Result<String> {
        match self.event().await? {
            Event::Attach {
                path,
                panel,
                output,
                zone,
                options,
                handle,
                ..
            } => {
                assert_eq!(path, self.desktop.display().to_string());
                assert_eq!(
                    (panel, output.as_str(), zone.as_str(), options.as_str()),
                    (0, "", "", "{}")
                );
                assert!(!self.root.path().join("data/applications").exists());
                Ok(handle)
            }
            other => anyhow::bail!("expected attach, got {other:?}"),
        }
    }

    async fn silent(&mut self, window: Duration) {
        let unexpected = tokio::time::timeout(window, self.events.recv()).await;
        assert!(unexpected.is_err(), "{unexpected:?}");
    }

    fn signal(&self, signal: i32) -> Result<()> {
        ensure!(
            unsafe {
                libc::kill(
                    self.command.as_ref().context("command stopped")?.id() as i32,
                    signal,
                )
            } == 0,
            "signal failed"
        );
        Ok(())
    }

    async fn stderr_contains(&self, text: &str) -> Result<()> {
        let path = self.root.path().join("command.stderr");
        tokio::time::timeout(Duration::from_secs(8), async {
            while !fs::read_to_string(&path).is_ok_and(|stderr| stderr.contains(text)) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .with_context(|| format!("command never printed {text:?}"))
    }

    async fn stop(&mut self, handle: &str) -> Result<()> {
        self.signal(libc::SIGTERM)?;
        assert_eq!(self.event().await?, Event::Detach(handle.to_owned()));
        assert!(self.output_after_exit().await?.status.success());
        Ok(())
    }

    async fn output_after_exit(&mut self) -> Result<std::process::Output> {
        tokio::time::timeout(Duration::from_secs(8), async {
            while self
                .command
                .as_mut()
                .context("command stopped")?
                .try_wait()?
                .is_none()
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("the command did not exit within 8 s")??;
        let mut output = self
            .command
            .take()
            .context("command stopped")?
            .wait_with_output()?;
        output.stderr = fs::read(self.root.path().join("command.stderr"))?;
        Ok(output)
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if let Some(mut child) = self.command.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_tree_command_reads_the_current_host_tree() -> Result<()> {
    let bus = Bus::start()?;
    let connection = zbus::connection::Builder::address(bus.address.as_str())?
        .build()
        .await?;
    let (tx, mut events) = mpsc::unbounded_channel();
    let host = Host {
        tx,
        attached: Arc::new(AtomicUsize::new(0)),
        refuse_attach: Arc::new(AtomicBool::new(false)),
        fail_restart: Arc::new(AtomicBool::new(false)),
    };
    connection
        .object_server()
        .at(APPLET_HOST_PATH, host)
        .await?;
    own_name(&connection, &applet_host_name("org.example.TreePanel")).await?;
    let root = tempfile::tempdir()?;
    fs::write(
        root.path().join(format!("{ID}.desktop")),
        "[Desktop Entry]\nType=Application\nName=Tree Test\nExec=/bin/true\nImplements=me.aresa.Glimpse.Applet1;\n",
    )?;
    fs::write(root.path().join("devconfig.toml"), "")?;
    let mut master = -1;
    let mut slave = -1;
    ensure!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
            )
        } == 0,
        "cannot create terminal"
    );
    let mut input = unsafe { fs::File::from_raw_fd(master) };
    let stdin = unsafe { fs::File::from_raw_fd(slave) };
    let mut child = Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .args(["--panel", "org.example.TreePanel", "applet", "run"])
        .arg(root.path())
        .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let result = async {
        let event = tokio::time::timeout(Duration::from_secs(8), events.recv())
            .await?
            .context("no attachment")?;
        let Event::Attach { handle, .. } = event else {
            anyhow::bail!("expected attachment")
        };
        input.write_all(b"t\n")?;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(8), events.recv())
                .await?
                .context("no tree request")?,
            Event::Tree(handle.clone())
        );
        unsafe {
            libc::kill(child.id() as i32, libc::SIGTERM);
        }
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(8), events.recv())
                .await?
                .context("no detach")?,
            Event::Detach(handle)
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if result.is_err() {
        let _ = child.kill();
    }
    let output = child.wait_with_output()?;
    result?;
    assert!(output.status.success());
    let expected = host_tree();
    assert!(serde_json::from_str::<serde_json::Value>(&expected).is_err());
    assert_eq!(String::from_utf8(output.stdout)?, format!("{expected}\n"));
    Ok(())
}

#[tokio::test]
async fn attach_carries_the_selection_options_env_and_stderr() -> Result<()> {
    let mut h = Harness::start(Setup {
        config: Some("options = { one = 1 }\nenv = { A = \"1\", B = \"2\" }\n"),
        extra: &[
            "--bar", "2", "--output", "DP-1", "--zone", "left", "--env", "B=3", "--env", "C=4",
        ],
        ..Setup::default()
    })
    .await?;
    match h.event().await? {
        Event::Attach {
            path,
            panel,
            output,
            zone,
            options,
            env,
            handle,
        } => {
            assert_eq!(path, h.desktop.display().to_string());
            assert_eq!((panel, output.as_str(), zone.as_str()), (2, "DP-1", "left"));
            assert_eq!(options, "{\"one\":1}");
            assert_eq!(
                env,
                HashMap::from([
                    ("A".to_owned(), "1".to_owned()),
                    ("B".to_owned(), "3".to_owned()),
                    ("C".to_owned(), "4".to_owned()),
                ])
            );
            assert_eq!(handle, "dev-0");
        }
        other => anyhow::bail!("expected attach, got {other:?}"),
    }
    assert!(!h.root.path().join("data/applications").exists());
    h.stderr_contains("host-wrote-stderr").await?;
    h.stderr_contains("attached to org.example.TestPanel.AppletHost as dev-0")
        .await?;
    h.stderr_contains("not watching files; pass --dev to restart on change")
        .await?;
    h.stop("dev-0").await
}

#[tokio::test]
async fn directory_with_justfile_builds_once_and_attaches_dist() -> Result<()> {
    for name in ["justfile", "Justfile", ".justfile"] {
        let mut h = Harness::start(Setup {
            justfile: Some((name, BUILD_RECIPE)),
            ..Setup::default()
        })
        .await?;
        let Event::Attach { path, handle, .. } = h.event().await? else {
            anyhow::bail!("expected attach")
        };
        assert_eq!(
            path,
            h.applet
                .join(format!("dist/{ID}.desktop"))
                .display()
                .to_string()
        );
        assert_eq!(fs::read_to_string(h.applet.join("dist/builds"))?, "x");
        h.stop(&handle).await?;
        assert_eq!(fs::read_to_string(h.applet.join("dist/builds"))?, "x");
    }
    Ok(())
}

const BUILD_RECIPE: &str = "build:\n    mkdir -p dist\n    cp org.example.DevTest.desktop dist/\n    printf x >> dist/builds\n";

#[tokio::test]
async fn directory_without_justfile_attaches_source() -> Result<()> {
    let mut h = Harness::start(Setup::default()).await?;
    let handle = h.attach().await?;
    h.stop(&handle).await
}

#[tokio::test]
async fn failed_startup_build_does_not_attach() -> Result<()> {
    let mut h = Harness::start(Setup {
        justfile: Some(("justfile", "build:\n    false\n")),
        ..Setup::default()
    })
    .await?;
    let output = h.output_after_exit().await?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("`just build` failed"));
    assert!(h.events.try_recv().is_err());
    Ok(())
}

#[tokio::test]
async fn dev_rebuilds_before_restart_and_source_desktop_reattach() -> Result<()> {
    let mut h = Harness::start(Setup {
        justfile: Some(("justfile", BUILD_RECIPE)),
        extra: &["--dev"],
        ..Setup::default()
    })
    .await?;
    let Event::Attach { path, handle, .. } = h.event().await? else {
        anyhow::bail!("expected attach")
    };
    let marker = h.applet.join("dist/builds");
    h.silent(Duration::from_millis(600)).await;
    assert_eq!(fs::read_to_string(&marker)?, "x");
    fs::write(h.applet.join("source.ts"), "changed")?;
    assert_eq!(h.event().await?, Event::Restart(handle.clone()));
    assert_eq!(fs::read_to_string(&marker)?, "xx");
    h.silent(Duration::from_millis(600)).await;
    fs::write(
        &h.desktop,
        fs::read_to_string(&h.desktop)?.replace("Dev Test", "Changed"),
    )?;
    assert_eq!(h.event().await?, Event::Detach(handle));
    let Event::Attach {
        path: next_path,
        handle,
        ..
    } = h.event().await?
    else {
        anyhow::bail!("expected reattach")
    };
    assert_eq!(next_path, path);
    assert!(fs::read_to_string(&next_path)?.contains("Name=Changed"));
    assert_eq!(fs::read_to_string(&marker)?, "xxx");
    h.silent(Duration::from_millis(600)).await;
    fs::write(h.applet.join("justfile"), "build:\n    false\n")?;
    h.stderr_contains("build failed, keeping the running applet")
        .await?;
    h.silent(Duration::from_millis(600)).await;
    assert_eq!(fs::read_to_string(&marker)?, "xxx");
    fs::write(h.applet.join("justfile"), BUILD_RECIPE)?;
    assert_eq!(h.event().await?, Event::Restart(handle.clone()));
    assert_eq!(fs::read_to_string(&marker)?, "xxxx");
    h.stop(&handle).await
}

#[tokio::test]
async fn source_edit_restarts_and_ignored_directory_does_not() -> Result<()> {
    let mut h = Harness::dev("", &[]).await?;
    let handle = h.attach().await?;
    h.stderr_contains("attached to").await?;
    fs::create_dir(h.applet.join("node_modules"))?;
    fs::write(h.applet.join("node_modules/x"), "ignored")?;
    fs::create_dir_all(h.applet.join("target/release"))?;
    fs::write(h.applet.join("target/release/.cargo-lock"), "locked")?;
    h.silent(Duration::from_millis(600)).await;
    fs::write(h.applet.join("source.ts"), "changed")?;
    assert_eq!(h.event().await?, Event::Restart(handle.clone()));
    h.stderr_contains("restarting: source.ts").await?;
    h.stop(&handle).await
}

#[tokio::test]
async fn run_without_dev_does_not_restart_after_a_source_edit() -> Result<()> {
    let mut h = Harness::start(Setup::default()).await?;
    let handle = h.attach().await?;
    h.stderr_contains("not watching files; pass --dev to restart on change")
        .await?;
    fs::write(h.applet.join("source.ts"), "changed")?;
    h.silent(Duration::from_millis(800)).await;
    h.stop(&handle).await
}

#[tokio::test]
async fn a_burst_of_edits_restarts_once_after_it_settles() -> Result<()> {
    let mut h = Harness::dev("", &[]).await?;
    let handle = h.attach().await?;
    for n in 0..5 {
        fs::write(h.applet.join("source.ts"), n.to_string())?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(h.event().await?, Event::Restart(handle.clone()));
    h.silent(Duration::from_millis(900)).await;
    h.stop(&handle).await
}

#[tokio::test]
async fn a_failed_restart_attaches_again() -> Result<()> {
    let mut h = Harness::dev("", &[]).await?;
    let handle = h.attach().await?;
    h.host.fail_restart.store(true, Ordering::SeqCst);
    fs::write(h.applet.join("source.ts"), "changed")?;
    assert_eq!(h.event().await?, Event::Restart(handle.clone()));
    assert_eq!(h.event().await?, Event::Detach(handle));
    let next = h.attach().await?;
    assert_eq!(next, "dev-1");
    h.stderr_contains("cannot restart dev-0").await?;
    h.stop("dev-1").await
}

#[tokio::test]
async fn a_failed_reattach_keeps_the_session_running() -> Result<()> {
    let mut h = Harness::dev("env = { A = \"1\" }\n", &[]).await?;
    let handle = h.attach().await?;
    h.host.refuse_attach.store(true, Ordering::SeqCst);
    fs::write(h.applet.join("devconfig.toml"), "env = { A = \"2\" }\n")?;
    assert_eq!(h.event().await?, Event::Detach(handle));
    h.stderr_contains("waiting for the next change").await?;
    assert!(
        h.command
            .as_mut()
            .context("command stopped")?
            .try_wait()?
            .is_none()
    );
    h.host.refuse_attach.store(false, Ordering::SeqCst);
    fs::write(h.applet.join("source.ts"), "changed")?;
    let next = h.attach().await?;
    assert_eq!(next, "dev-1");
    h.stop(&next).await
}

#[tokio::test]
async fn a_reserved_env_flag_is_refused() -> Result<()> {
    let mut h = Harness::start(Setup {
        extra: &["--env", "NOTIFY_SOCKET=/x"],
        ..Setup::default()
    })
    .await?;
    let output = h.output_after_exit().await?;
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("--env name is not allowed: \"NOTIFY_SOCKET\"")
    );
    assert!(h.events.try_recv().is_err());
    Ok(())
}

#[tokio::test]
async fn options_edit_calls_set_options_without_restarting() -> Result<()> {
    let mut h = Harness::dev("options = { one = 1 }\n", &[]).await?;
    match h.event().await? {
        Event::Attach { options, .. } => assert_eq!(options, "{\"one\":1}"),
        other => anyhow::bail!("{other:?}"),
    }
    fs::write(h.applet.join("devconfig.toml"), "")?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    fs::write(h.applet.join("devconfig.toml"), "options = { two = 2 }\n")?;
    assert_eq!(
        h.event().await?,
        Event::Options("dev-0".to_owned(), "{\"two\":2}".to_owned())
    );
    h.silent(Duration::from_millis(600)).await;
    h.stop("dev-0").await
}

#[tokio::test]
async fn env_edit_detaches_and_attaches_again() -> Result<()> {
    let mut h = Harness::dev("env = { A = \"1\" }\n", &[]).await?;
    h.attach().await?;
    fs::write(h.applet.join("devconfig.toml"), "env = { A = \"2\" }\n")?;
    assert_eq!(h.event().await?, Event::Detach("dev-0".to_owned()));
    match h.event().await? {
        Event::Attach { env, handle, .. } => {
            assert_eq!(env, HashMap::from([("A".to_owned(), "2".to_owned())]));
            assert_eq!(handle, "dev-1");
        }
        other => anyhow::bail!("{other:?}"),
    }
    h.stop("dev-1").await
}

#[tokio::test]
async fn applet_config_outside_the_applet_is_read_and_watched() -> Result<()> {
    let settings = tempfile::tempdir()?;
    let config = settings.path().join("dev.toml");
    fs::write(&config, "options = { one = 1 }\n")?;
    let config_arg = config.to_str().context("utf-8 path")?;
    let mut h = Harness::start(Setup {
        config: None,
        extra: &["--dev", "--applet-config", config_arg],
        ..Setup::default()
    })
    .await?;
    match h.event().await? {
        Event::Attach { options, .. } => assert_eq!(options, "{\"one\":1}"),
        other => anyhow::bail!("{other:?}"),
    }
    fs::write(&config, "options = { two = 2 }\n")?;
    assert_eq!(
        h.event().await?,
        Event::Options("dev-0".to_owned(), "{\"two\":2}".to_owned())
    );
    h.stop("dev-0").await
}

#[tokio::test]
async fn applet_config_naming_another_id_is_refused() -> Result<()> {
    let mut h = Harness::dev("applet = \"org.example.Other\"\n", &[]).await?;
    let output = h.output_after_exit().await?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("configures org.example.Other, not org.example.DevTest")
    );
    assert!(h.events.try_recv().is_err());
    Ok(())
}

#[tokio::test]
async fn a_stale_sandbox_table_is_refused() -> Result<()> {
    let mut h = Harness::dev("[sandbox]\nread = [\".\"]\n", &[]).await?;
    let output = h.output_after_exit().await?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("sandbox"));
    assert!(h.events.try_recv().is_err());
    Ok(())
}

#[tokio::test]
async fn missing_config_toml_fails_with_a_hint() -> Result<()> {
    let mut h = Harness::start(Setup {
        config: None,
        ..Setup::default()
    })
    .await?;
    let output = h.output_after_exit().await?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains(&format!(
        "no devconfig.toml in {}; create one or pass --applet-config <file>",
        h.applet.display()
    )));
    assert!(h.events.try_recv().is_err());
    Ok(())
}

#[tokio::test]
async fn missing_config_toml_fails_before_building() -> Result<()> {
    let mut h = Harness::start(Setup {
        config: None,
        justfile: Some(("justfile", BUILD_RECIPE)),
        ..Setup::default()
    })
    .await?;
    let output = h.output_after_exit().await?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no devconfig.toml"));
    assert!(!h.applet.join("dist").exists());
    Ok(())
}

#[tokio::test]
async fn an_env_only_change_restarts_without_building() -> Result<()> {
    let mut h = Harness::start(Setup {
        config: Some("env = { A = \"1\" }\n"),
        justfile: Some(("justfile", BUILD_RECIPE)),
        extra: &["--dev"],
        ..Setup::default()
    })
    .await?;
    let Event::Attach { handle, .. } = h.event().await? else {
        anyhow::bail!("expected attach")
    };
    fs::write(h.applet.join("devconfig.toml"), "env = { A = \"2\" }\n")?;
    assert_eq!(h.event().await?, Event::Detach(handle));
    let Event::Attach { handle: next, .. } = h.event().await? else {
        anyhow::bail!("expected attach")
    };
    assert_eq!(fs::read_to_string(h.applet.join("dist/builds"))?, "x");
    h.stop(&next).await
}

#[tokio::test]
async fn refused_attach_reports_the_reason_and_fails() -> Result<()> {
    let mut h = Harness::start(Setup {
        refuse_attach: true,
        ..Setup::default()
    })
    .await?;
    let output = h.output_after_exit().await?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("is already attached"));
    Ok(())
}

#[tokio::test]
async fn absent_panel_fails_with_a_clear_error() -> Result<()> {
    let mut h = Harness::start(Setup {
        serve: false,
        ..Setup::default()
    })
    .await?;
    let output = h.output_after_exit().await?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains(
        "no panel is running: org.example.TestPanel.AppletHost is not on the session bus"
    ));
    Ok(())
}

#[tokio::test]
async fn the_sandbox_and_trace_flags_are_gone() -> Result<()> {
    for flag in ["--no-sandbox", "--trace"] {
        let mut h = Harness::start(Setup {
            extra: &[flag],
            ..Setup::default()
        })
        .await?;
        let output = h.output_after_exit().await?;
        assert_eq!(output.status.code(), Some(2), "{flag}");
        assert!(h.events.try_recv().is_err());
    }
    Ok(())
}
