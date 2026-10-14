use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsFd;
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
}

#[derive(Clone)]
struct Host {
    tx: mpsc::UnboundedSender<Event>,
    attached: Arc<AtomicUsize>,
    refuse_attach: Arc<AtomicBool>,
    fail_restart: Arc<AtomicBool>,
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
    extra: &'a [&'a str],
    refuse_attach: bool,
    serve: bool,
}

impl Default for Setup<'_> {
    fn default() -> Self {
        Self {
            config: Some(""),
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
            fs::write(applet.join("config.toml"), config)?;
        }
        let child = Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
            .args(["applets", "run"])
            .arg(&applet)
            .args(setup.extra)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
            .env("GLIMPSE_PANEL_APP_ID", "org.example.TestPanel")
            .env("XDG_DATA_HOME", root.path().join("data"))
            .stdout(Stdio::piped())
            .stderr(fs::File::create(root.path().join("command.stderr"))?)
            .spawn()?;
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
            .await?
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
        .await??;
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

#[tokio::test]
async fn attach_carries_the_selection_options_env_and_stderr() -> Result<()> {
    let mut h = Harness::start(Setup {
        config: Some("options = { one = 1 }\nenv = { A = \"1\", B = \"2\" }\n"),
        extra: &[
            "--panel", "2", "--output", "DP-1", "--zone", "left", "--env", "B=3", "--env", "C=4",
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
async fn source_edit_restarts_and_ignored_directory_does_not() -> Result<()> {
    let mut h = Harness::dev("", &[]).await?;
    let handle = h.attach().await?;
    h.stderr_contains("attached to").await?;
    fs::create_dir(h.applet.join("node_modules"))?;
    fs::write(h.applet.join("node_modules/x"), "ignored")?;
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
    assert_eq!(h.event().await?, Event::Restart(handle));
    let next = h.attach().await?;
    assert_eq!(next, "dev-1");
    h.stderr_contains("cannot restart dev-0").await?;
    h.stop("dev-1").await
}

#[tokio::test]
async fn options_edit_calls_set_options_without_restarting() -> Result<()> {
    let mut h = Harness::dev("options = { one = 1 }\n", &[]).await?;
    match h.event().await? {
        Event::Attach { options, .. } => assert_eq!(options, "{\"one\":1}"),
        other => anyhow::bail!("{other:?}"),
    }
    fs::write(h.applet.join("config.toml"), "options = { two = 2 }\n")?;
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
    fs::write(h.applet.join("config.toml"), "env = { A = \"2\" }\n")?;
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
        "no config.toml in {}; create one or pass --applet-config <file>",
        h.applet.display()
    )));
    assert!(h.events.try_recv().is_err());
    Ok(())
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
