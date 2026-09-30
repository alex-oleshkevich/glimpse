use std::fs;
use std::io::{BufRead, BufReader};
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
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};
use tokio::net::unix::pipe::{Receiver, Sender};
use tokio::sync::{Mutex, mpsc};
use zbus::zvariant::Fd;

const ID: &str = "org.example.DevTest";
const HELLO: &str = r#"{"t":"hello","v":1}"#;
const HOST_HELLO: &str = r#"{"t":"hello","v":1,"name":"Dev Test","state":"/tmp","options":{},"placement":{"output":null,"position":"top","orientation":"horizontal","zone":"right","size":32}}"#;

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

#[derive(Debug)]
enum Event {
    Attach {
        path: String,
        panel: u32,
        output: String,
        zone: String,
        options: String,
        slot: usize,
    },
    AttachFailed,
    Line {
        slot: usize,
        line: String,
    },
    Eof {
        slot: usize,
    },
    Options(String),
}

struct Slot {
    writer: Arc<Mutex<Sender>>,
    reader: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
struct Host {
    tx: mpsc::UnboundedSender<Event>,
    slots: Arc<Mutex<Vec<Option<Slot>>>>,
    close_on_attach: Arc<AtomicBool>,
    fail_next_attach: Arc<AtomicBool>,
    refuse_duplicate: Arc<AtomicBool>,
    attach_calls: Arc<AtomicUsize>,
}

impl Host {
    async fn reply(&self, slot: usize) -> Result<()> {
        let writer = self.slots.lock().await[slot]
            .as_ref()
            .context("closed slot")?
            .writer
            .clone();
        let mut writer = writer.lock().await;
        writer.write_all(HOST_HELLO.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        Ok(())
    }

    async fn close(&self, slot: usize) {
        if let Some(slot) = self.slots.lock().await[slot].take() {
            slot.reader.abort();
        }
    }

    async fn close_writer(&self, slot: usize) {
        self.slots.lock().await[slot].take();
    }
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
        to_applet: Fd<'_>,
        from_applet: Fd<'_>,
    ) -> zbus::fdo::Result<String> {
        self.attach_calls.fetch_add(1, Ordering::SeqCst);
        if self.refuse_duplicate.load(Ordering::SeqCst) {
            let _ = self.tx.send(Event::AttachFailed);
            return Err(zbus::fdo::Error::Failed(
                "org.example.DevTest is already attached".to_owned(),
            ));
        }
        if self.fail_next_attach.swap(false, Ordering::SeqCst) {
            let _ = self.tx.send(Event::AttachFailed);
            return Err(zbus::fdo::Error::Failed(
                "one-time attach failure".to_owned(),
            ));
        }
        let to_applet = to_applet
            .as_fd()
            .try_clone_to_owned()
            .map_err(|error| zbus::fdo::Error::IOError(error.to_string()))?;
        let from_applet = from_applet
            .as_fd()
            .try_clone_to_owned()
            .map_err(|error| zbus::fdo::Error::IOError(error.to_string()))?;
        let writer = Arc::new(Mutex::new(
            Sender::from_owned_fd(to_applet)
                .map_err(|error| zbus::fdo::Error::IOError(error.to_string()))?,
        ));
        let receiver = Receiver::from_owned_fd(from_applet)
            .map_err(|error| zbus::fdo::Error::IOError(error.to_string()))?;
        let tx = self.tx.clone();
        let mut slots = self.slots.lock().await;
        let slot = slots.len();
        let reader = tokio::spawn(async move {
            let mut lines = AsyncBufReader::new(receiver).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(Event::Line { slot, line });
            }
            let _ = tx.send(Event::Eof { slot });
        });
        slots.push(Some(Slot { writer, reader }));
        let _ = self.tx.send(Event::Attach {
            path: desktop_file.to_owned(),
            panel,
            output: output.to_owned(),
            zone: zone.to_owned(),
            options: options.to_owned(),
            slot,
        });
        drop(slots);
        if self.close_on_attach.load(Ordering::SeqCst) {
            self.close(slot).await;
        }
        Ok(format!("dev-{slot}"))
    }

    async fn set_options(&self, _handle: &str, options: &str) -> zbus::fdo::Result<()> {
        let _ = self.tx.send(Event::Options(options.to_owned()));
        Ok(())
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
    async fn start(script: &str, config: &str, extra: &[&str]) -> Result<Self> {
        let mut args = vec!["--dev"];
        args.extend_from_slice(extra);
        Self::start_command("run", script, config, &args).await
    }

    async fn start_command(
        command_name: &str,
        script: &str,
        config: &str,
        extra: &[&str],
    ) -> Result<Self> {
        Self::start_command_with_refusal(command_name, script, config, extra, false).await
    }

    async fn start_command_with_refusal(
        command_name: &str,
        script: &str,
        config: &str,
        extra: &[&str],
        refuse_duplicate: bool,
    ) -> Result<Self> {
        Self::start_command_with_config(command_name, script, config, extra, refuse_duplicate, true)
            .await
    }

    async fn start_command_with_config(
        command_name: &str,
        script: &str,
        config: &str,
        extra: &[&str],
        refuse_duplicate: bool,
        write_config: bool,
    ) -> Result<Self> {
        let bus = Bus::start()?;
        let connection = zbus::connection::Builder::address(bus.address.as_str())?
            .build()
            .await?;
        let (tx, events) = mpsc::unbounded_channel();
        let host = Host {
            tx,
            slots: Arc::new(Mutex::new(Vec::new())),
            close_on_attach: Arc::new(AtomicBool::new(false)),
            fail_next_attach: Arc::new(AtomicBool::new(false)),
            refuse_duplicate: Arc::new(AtomicBool::new(refuse_duplicate)),
            attach_calls: Arc::new(AtomicUsize::new(0)),
        };
        connection
            .object_server()
            .at(APPLET_HOST_PATH, host.clone())
            .await?;
        own_name(&connection, &applet_host_name("org.example.TestPanel")).await?;
        let root = tempfile::tempdir()?;
        let applet = root.path().join("applet");
        fs::create_dir(&applet)?;
        let script_path = applet.join("main.sh");
        fs::write(&script_path, script)?;
        let desktop = applet.join(format!("{ID}.desktop"));
        fs::write(
            &desktop,
            format!(
                "[Desktop Entry]\nType=Application\nName=Dev Test\nExec=/bin/sh {}\nNoDisplay=true\nImplements=me.aresa.Glimpse.Applet1\n",
                script_path.display()
            ),
        )?;
        let mut command = Command::new(env!("CARGO_BIN_EXE_glimpsectl"));
        command.args(["applets", command_name]).arg(&applet);
        if write_config {
            fs::write(applet.join("config.toml"), config)?;
        }
        command
            .args(extra)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
            .env("GLIMPSE_PANEL_APP_ID", "org.example.TestPanel")
            .env("XDG_DATA_HOME", root.path().join("data"))
            .env("XDG_STATE_HOME", root.path().join("state"))
            .stdout(Stdio::piped())
            .stderr(fs::File::create(root.path().join("command.stderr"))?);
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
            .await?
            .context("event stream closed")
    }

    async fn attach(&mut self) -> Result<usize> {
        match self.event().await? {
            Event::Attach {
                path,
                panel,
                output,
                zone,
                options,
                slot,
            } => {
                assert_eq!(path, self.desktop.display().to_string());
                assert_eq!(
                    (panel, output.as_str(), zone.as_str(), options.as_str()),
                    (0, "", "", "{}")
                );
                assert!(!self.root.path().join("data/applications").exists());
                Ok(slot)
            }
            other => anyhow::bail!("expected attach, got {other:?}"),
        }
    }

    async fn hello(&mut self, slot: usize) -> Result<()> {
        match self.event().await? {
            Event::Line { slot: from, line } if from == slot && line == HELLO => Ok(()),
            other => anyhow::bail!("expected hello, got {other:?}"),
        }
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

    fn finish(mut self, signal: i32) -> Result<std::process::Output> {
        self.signal(signal)?;
        let mut output = self
            .command
            .take()
            .context("command stopped")?
            .wait_with_output()?;
        output.stderr = fs::read(self.root.path().join("command.stderr"))?;
        Ok(output)
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
async fn attaches_without_installing_and_forwards_stderr() -> Result<()> {
    let mut h = Harness::start(
        &format!("printf '%s\\n' '{HELLO}'\necho applet-stderr >&2\nread -r line\nexec sleep 30\n"),
        "",
        &["--no-sandbox"],
    )
    .await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    h.host.reply(slot).await?;
    h.stderr_contains("attached to org.example.TestPanel.AppletHost as dev-0")
        .await?;
    h.stderr_contains("applet started (pid ").await?;
    h.stderr_contains("applet ready").await?;
    let output = h.finish(libc::SIGTERM)?;
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("applet-stderr"));
    Ok(())
}

#[tokio::test]
async fn source_edit_restarts_and_ignored_directory_does_not() -> Result<()> {
    let mut h = Harness::start(
        &format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n"),
        "",
        &["--no-sandbox"],
    )
    .await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    h.host.reply(slot).await?;
    h.stderr_contains("attached to org.example.TestPanel.AppletHost as dev-0")
        .await?;
    h.stderr_contains("applet started (pid ").await?;
    h.stderr_contains("applet ready").await?;
    fs::create_dir(h.applet.join("node_modules"))?;
    fs::write(h.applet.join("node_modules/x"), "ignored")?;
    let unexpected = tokio::time::timeout(Duration::from_millis(500), h.events.recv()).await;
    assert!(unexpected.is_err(), "{unexpected:?}");
    fs::write(h.applet.join("source.ts"), "changed")?;
    h.hello(slot).await?;
    h.host.reply(slot).await?;
    h.stderr_contains("restarting: source.ts").await?;
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}

#[tokio::test]
async fn run_does_not_restart_after_a_source_edit() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n");
    let mut h = Harness::start_command("run", &script, "", &["--no-sandbox"]).await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    h.host.reply(slot).await?;
    h.stderr_contains("not watching files; pass --dev to restart on change")
        .await?;
    fs::write(h.applet.join("source.ts"), "changed")?;
    let unexpected = tokio::time::timeout(Duration::from_millis(600), h.events.recv()).await;
    assert!(unexpected.is_err(), "{unexpected:?}");
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}

#[tokio::test]
async fn run_exits_non_zero_when_the_applet_exits() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nexit 3\n");
    let mut h = Harness::start_command("run", &script, "", &["--no-sandbox"]).await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    let output = h.output_after_exit().await?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("applet exited (exit status: 3)"));
    Ok(())
}

#[tokio::test]
async fn dev_reports_applet_exit_and_waits_for_a_change() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nexit 3\n");
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    h.stderr_contains("applet exited (exit status: 3); waiting for a change")
        .await?;
    assert!(
        h.command
            .as_mut()
            .context("command stopped")?
            .try_wait()?
            .is_none()
    );
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}

#[tokio::test]
async fn run_reports_duplicate_attach_once() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\n");
    let mut h =
        Harness::start_command_with_refusal("run", &script, "", &["--no-sandbox"], true).await?;
    assert!(matches!(h.event().await?, Event::AttachFailed));
    let output = h.output_after_exit().await?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("already attached"));
    assert_eq!(h.host.attach_calls.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn run_stops_after_duplicate_reattach_refusal() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n");
    let mut h = Harness::start_command("run", &script, "", &["--no-sandbox"]).await?;
    let first = h.attach().await?;
    h.hello(first).await?;
    h.host.refuse_duplicate.store(true, Ordering::SeqCst);
    h.host.close(first).await;
    assert!(matches!(h.event().await?, Event::AttachFailed));
    let output = h.output_after_exit().await?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("already attached"));
    assert_eq!(h.host.attach_calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn applet_config_naming_another_id_is_refused() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n");
    let mut h = Harness::start(
        &script,
        "applet = \"org.example.Other\"\n",
        &["--no-sandbox"],
    )
    .await?;
    tokio::time::timeout(Duration::from_secs(8), async {
        while h
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
    let mut output = h
        .command
        .take()
        .context("command stopped")?
        .wait_with_output()?;
    output.stderr = fs::read(h.root.path().join("command.stderr"))?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("configures org.example.Other, not org.example.DevTest")
    );
    assert!(h.events.try_recv().is_err());
    Ok(())
}

#[tokio::test]
async fn rejected_commit_never_reaches_the_host() -> Result<()> {
    let invalid = r#"{"t":"commit","ops":[{"op":"insert","parent":0,"node":{"id":1,"type":"indicator"}},{"op":"insert","parent":999,"node":{"id":2,"type":"label"}}]}"#;
    let script = format!(
        "printf '%s\\n' '{HELLO}'\nread -r line\nprintf '%s\\n' '{invalid}'\nexec sleep 30\n"
    );
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    h.host.reply(slot).await?;
    let unexpected = tokio::time::timeout(Duration::from_millis(400), h.events.recv()).await;
    assert!(unexpected.is_err(), "{unexpected:?}");
    let output = h.finish(libc::SIGTERM)?;
    assert!(String::from_utf8_lossy(&output.stderr).contains("commit rejected:"));
    Ok(())
}

#[tokio::test]
async fn options_edit_calls_set_options_without_restarting() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n");
    let mut h = Harness::start(&script, "options = { one = 1 }\n", &["--no-sandbox"]).await?;
    let slot = match h.event().await? {
        Event::Attach { slot, options, .. } => {
            assert_eq!(options, "{\"one\":1}");
            slot
        }
        other => anyhow::bail!("{other:?}"),
    };
    h.hello(slot).await?;
    h.host.reply(slot).await?;
    fs::write(h.applet.join("config.toml"), "options = { two = 2 }\n")?;
    match h.event().await? {
        Event::Options(options) => assert_eq!(options, "{\"two\":2}"),
        other => anyhow::bail!("{other:?}"),
    }
    let unexpected = tokio::time::timeout(Duration::from_millis(500), h.events.recv()).await;
    assert!(unexpected.is_err(), "{unexpected:?}");
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}

#[tokio::test]
async fn applet_config_outside_the_applet_is_read_and_watched() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n");
    let settings = tempfile::tempdir()?;
    let config = settings.path().join("dev.toml");
    fs::write(&config, "options = { one = 1 }\n")?;
    let config_arg = config.to_str().context("utf-8 path")?;
    let mut h = Harness::start(
        &script,
        "",
        &["--no-sandbox", "--applet-config", config_arg],
    )
    .await?;
    let slot = match h.event().await? {
        Event::Attach { slot, options, .. } => {
            assert_eq!(options, "{\"one\":1}");
            slot
        }
        other => anyhow::bail!("{other:?}"),
    };
    h.hello(slot).await?;
    h.host.reply(slot).await?;
    fs::write(&config, "options = { two = 2 }\n")?;
    match h.event().await? {
        Event::Options(options) => assert_eq!(options, "{\"two\":2}"),
        other => anyhow::bail!("{other:?}"),
    }
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}

#[tokio::test]
async fn config_toml_next_to_the_applet_is_found_without_the_flag() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n");
    let mut h = Harness::start(&script, "options = { one = 1 }\n", &["--no-sandbox"]).await?;
    let slot = match h.event().await? {
        Event::Attach { slot, options, .. } => {
            assert_eq!(options, "{\"one\":1}");
            slot
        }
        other => anyhow::bail!("expected attach, got {other:?}"),
    };
    h.hello(slot).await?;
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}

#[tokio::test]
async fn missing_config_toml_fails_with_a_hint() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n");
    let mut h =
        Harness::start_command_with_config("run", &script, "", &["--no-sandbox"], false, false)
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
async fn dropped_slot_reattaches_and_term_stops_the_child() -> Result<()> {
    let script = format!(
        "echo $$ > \"$XDG_STATE_HOME/pid\"\nprintf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n"
    );
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let first = h.attach().await?;
    h.hello(first).await?;
    h.host.reply(first).await?;
    h.host.close(first).await;
    let second = h.attach().await?;
    h.hello(second).await?;
    let pid: i32 = fs::read_to_string(h.root.path().join("state/pid"))?
        .trim()
        .parse()?;
    let output = h.finish(libc::SIGTERM)?;
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("panel dropped the applet"));
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    Ok(())
}

#[tokio::test]
async fn repeated_slot_closure_obeys_the_restart_limit() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n");
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let first = h.attach().await?;
    h.hello(first).await?;
    h.host.close_on_attach.store(true, Ordering::SeqCst);
    h.host.close(first).await;
    let mut attaches = 1;
    let excess = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if matches!(h.events.recv().await, Some(Event::Attach { .. })) {
                attaches += 1;
                if attaches > glimpse_services::EXEC_MAX_HELLOS {
                    break;
                }
            }
        }
    })
    .await;
    assert!(excess.is_err(), "{attaches} attach calls");
    assert!(attaches > 1);
    assert!(
        h.command
            .as_mut()
            .context("command stopped")?
            .try_wait()?
            .is_none()
    );
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}

#[tokio::test]
async fn failed_reattach_retries_while_the_panel_owns_its_name() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n");
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let first = h.attach().await?;
    h.hello(first).await?;
    h.host.fail_next_attach.store(true, Ordering::SeqCst);
    h.host.close(first).await;
    assert!(matches!(h.event().await?, Event::AttachFailed));
    let second = h.attach().await?;
    h.hello(second).await?;
    let output = h.finish(libc::SIGTERM)?;
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr)
            .matches("cannot attach applet:")
            .count(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn new_panel_link_forgets_the_old_hello_barrier() -> Result<()> {
    let script = format!(
        "printf '%s\\n' '{HELLO}'\nread -r line\nprintf '%s\\n' '{{\"t\":\"notify\",\"summary\":\"mounted\"}}'\nexec sleep 30\n"
    );
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let first = h.attach().await.context("initial attach")?;
    h.hello(first).await.context("initial hello")?;
    h.host.close(first).await;
    h.stderr_contains("panel dropped the applet, re-attaching")
        .await
        .context("pipe closure")?;
    let second = h.attach().await.with_context(|| {
        format!(
            "reattach after pipe close; stderr: {}",
            fs::read_to_string(h.root.path().join("command.stderr")).unwrap_or_default()
        )
    })?;
    h.hello(second).await.context("hello after pipe close")?;
    h.host.reply(second).await?;
    match h.event().await.context("notification after new hello")? {
        Event::Line { slot, line } => {
            assert_eq!(slot, second);
            assert!(line.contains("mounted"), "{line}");
        }
        other => anyhow::bail!("expected mounted notification, got {other:?}"),
    }
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}

#[tokio::test]
async fn unsolicited_panel_hello_is_not_forwarded() -> Result<()> {
    let script = format!(
        "printf '%s\\n' '{HELLO}'\nread -r line\nread -r line\nprintf '%s\\n' '{{\"t\":\"notify\",\"summary\":\"duplicate\"}}'\nexec sleep 30\n"
    );
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    h.host.reply(slot).await?;
    h.host.reply(slot).await?;
    let unexpected = tokio::time::timeout(Duration::from_millis(300), h.events.recv()).await;
    assert!(unexpected.is_err(), "{unexpected:?}");
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}

#[tokio::test]
async fn unsupported_applet_hello_is_refused_not_relayed() -> Result<()> {
    let script = "printf '%s\\n' '{\"t\":\"hello\",\"v\":2}'\nexec sleep 30\n";
    let mut h = Harness::start(script, "", &["--no-sandbox"]).await?;
    h.attach().await?;
    h.stderr_contains("unsupported applet protocol version 2")
        .await?;
    let unexpected = tokio::time::timeout(Duration::from_millis(300), h.events.recv()).await;
    assert!(unexpected.is_err(), "{unexpected:?}");
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}

#[tokio::test]
async fn restart_and_ctrl_c_stop_grandchildren() -> Result<()> {
    let script = format!(
        "sleep 30 &\necho $! > \"$XDG_STATE_HOME/grandchild\"\nprintf '%s\\n' '{HELLO}'\nread -r line\nwait\n"
    );
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    let grandchild: i32 = fs::read_to_string(h.root.path().join("state/grandchild"))?
        .trim()
        .parse()?;
    assert_eq!(unsafe { libc::kill(grandchild, 0) }, 0);
    fs::write(h.applet.join("source.ts"), "changed")?;
    h.hello(slot).await?;
    grandchild_gone(grandchild).await?;
    let restarted: i32 = fs::read_to_string(h.root.path().join("state/grandchild"))?
        .trim()
        .parse()?;
    assert_ne!(grandchild, restarted);
    assert_eq!(unsafe { libc::kill(restarted, 0) }, 0);
    h.signal(libc::SIGINT)?;
    loop {
        if let Event::Eof { slot: closed } = h.event().await? {
            assert_eq!(closed, slot);
            break;
        }
    }
    assert!(h.finish(libc::SIGTERM)?.status.success());
    grandchild_gone(restarted).await?;

    let script = format!(
        "sleep 30 </dev/null >/dev/null 2>&1 &\necho $! > \"$XDG_STATE_HOME/grandchild\"\nprintf '%s\\n' '{HELLO}'\nexit 0\n"
    );
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    let grandchild: i32 = fs::read_to_string(h.root.path().join("state/grandchild"))?
        .trim()
        .parse()?;
    h.stderr_contains("applet exited (exit status: 0); waiting for a change")
        .await?;
    grandchild_gone(grandchild).await?;
    fs::write(h.applet.join("source.ts"), "changed")?;
    h.hello(slot).await?;
    let restarted: i32 = fs::read_to_string(h.root.path().join("state/grandchild"))?
        .trim()
        .parse()?;
    assert_ne!(grandchild, restarted);
    assert!(h.finish(libc::SIGTERM)?.status.success());
    grandchild_gone(restarted).await?;
    Ok(())
}

async fn grandchild_gone(pid: i32) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if unsafe { libc::kill(pid, 0) } == -1
                || fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
                    stat.split_once(") ")
                        .is_some_and(|(_, rest)| rest.starts_with('Z'))
                })
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .with_context(|| format!("grandchild {pid} survived"))
}

#[tokio::test]
async fn panel_name_returning_reattaches_after_its_pipes_close() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n");
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let first = h.attach().await.context("initial attach")?;
    h.hello(first).await.context("initial hello")?;
    let name = applet_host_name("org.example.TestPanel");
    h._connection.release_name(name.as_str()).await?;
    h.host.close_writer(first).await;
    assert!(
        matches!(h.event().await.context("first pipe EOF")?, Event::Eof { slot } if slot == first)
    );
    h.stderr_contains("panel went away, waiting for it")
        .await
        .context("owner loss")?;
    own_name(&h._connection, &name).await?;
    let second = h.attach().await.with_context(|| {
        format!(
            "reattach after owner return; stderr: {}",
            fs::read_to_string(h.root.path().join("command.stderr")).unwrap_or_default()
        )
    })?;
    h.hello(second).await.context("hello after owner return")?;
    let output = h.finish(libc::SIGTERM)?;
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("panel went away, waiting for it"));
    Ok(())
}

#[tokio::test]
async fn stale_host_hello_is_not_delivered_to_the_restarted_child() -> Result<()> {
    let script = format!(
        "printf '%s\\n' '{HELLO}'\nread -r line\nprintf '%s\\n' '{{\"t\":\"notify\",\"summary\":\"mounted\"}}'\nread -r line\nprintf '%s\\n' '{{\"t\":\"notify\",\"summary\":\"duplicate\"}}'\n"
    );
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    fs::write(h.applet.join("source.ts"), "changed")?;
    h.hello(slot).await?;
    h.host.reply(slot).await?;
    assert!(
        tokio::time::timeout(Duration::from_millis(200), h.events.recv())
            .await
            .is_err()
    );
    h.host.reply(slot).await?;
    match h.event().await? {
        Event::Line { line, .. } => assert!(line.contains("mounted"), "{line}"),
        other => anyhow::bail!("{other:?}"),
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(200), h.events.recv())
            .await
            .is_err()
    );
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}

#[tokio::test]
async fn rapid_edits_defer_a_restart_before_the_panel_hello_cap() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n");
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    for n in 0..10 {
        fs::write(h.applet.join("source.ts"), n.to_string())?;
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    tokio::time::sleep(Duration::from_millis(350)).await;
    let mut hellos = 1;
    while let Ok(event) = h.events.try_recv() {
        if let Event::Line { slot: from, line } = event {
            assert_eq!(from, slot);
            assert_eq!(line, HELLO);
            hellos += 1;
        }
    }
    assert!(
        hellos <= glimpse_services::EXEC_MAX_HELLOS,
        "{hellos} hellos"
    );
    tokio::time::sleep(glimpse_services::EXEC_HELLO_WINDOW + Duration::from_millis(500)).await;
    let mut deferred_hellos = 0;
    while let Ok(event) = h.events.try_recv() {
        if let Event::Line { slot: from, line } = event {
            assert_eq!(from, slot);
            assert_eq!(line, HELLO);
            deferred_hellos += 1;
        }
    }
    assert_eq!(deferred_hellos, 1);
    let output = h.finish(libc::SIGTERM)?;
    assert!(String::from_utf8_lossy(&output.stderr).contains("reload deferred"));
    Ok(())
}

#[tokio::test]
async fn a_burst_of_edits_restarts_once_after_it_settles() -> Result<()> {
    let script = format!("printf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n");
    let mut h = Harness::start(&script, "", &["--no-sandbox"]).await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    for n in 0..5 {
        fs::write(h.applet.join("source.ts"), n.to_string())?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_millis(900)).await;
    let mut hellos = 0;
    while let Ok(event) = h.events.try_recv() {
        if let Event::Line { line, .. } = event
            && line == HELLO
        {
            hellos += 1;
        }
    }
    assert_eq!(hellos, 1);
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}

#[tokio::test]
async fn sandbox_denies_an_ungranted_file_and_no_sandbox_allows_it() -> Result<()> {
    let abi = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<u8>(),
            0,
            1,
        )
    };
    if abi < 10 {
        eprintln!("skipping Landlock test: ABI {abi} below 10");
        return Ok(());
    }
    let outside = tempfile::tempdir()?;
    let secret = outside.path().join("secret");
    fs::write(&secret, "visible")?;
    let env = format!("SECRET_PATH={}", secret.display());
    let script = format!(
        "if cat \"$SECRET_PATH\" >/dev/null 2>&1; then echo read-allowed >&2; else echo read-denied >&2; fi\nprintf '%s\\n' '{HELLO}'\nread -r line\n"
    );
    let config = "[sandbox]\nread = [\".\"]\nwrite = [\".\"]\n";
    for (flags, expected) in [
        (["--env", env.as_str()].as_slice(), "read-denied"),
        (
            ["--no-sandbox", "--env", env.as_str()].as_slice(),
            "read-allowed",
        ),
    ] {
        let mut h = Harness::start(&script, config, flags).await?;
        let slot = h.attach().await?;
        h.hello(slot).await?;
        let output = h.finish(libc::SIGTERM)?;
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[tokio::test]
async fn run_can_read_and_write_its_own_directory_with_sandbox() -> Result<()> {
    let abi = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<u8>(),
            0,
            1,
        )
    };
    if abi < 10 {
        eprintln!("skipping Landlock test: ABI {abi} below 10");
        return Ok(());
    }
    let script = format!(
        "cat \"$0\" >/dev/null && mkdir -p \"$(dirname \"$0\")/build\" && printf built > \"$(dirname \"$0\")/build/output\" && echo source-ok >&2\nprintf '%s\\n' '{HELLO}'\nread -r line\nexec sleep 30\n"
    );
    let mut h = Harness::start_command("run", &script, "", &[]).await?;
    let slot = h.attach().await?;
    h.hello(slot).await?;
    h.stderr_contains("source-ok").await?;
    assert_eq!(fs::read_to_string(h.applet.join("build/output"))?, "built");
    assert!(h.finish(libc::SIGTERM)?.status.success());
    Ok(())
}
