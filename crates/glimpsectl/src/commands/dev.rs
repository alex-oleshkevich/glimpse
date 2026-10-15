use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use glimpse_config::{Config, Panel, runtime_dir};
use glimpse_dbus::applet_host_name;
use tokio::signal::unix::{SignalKind, signal};
use zbus::export::futures_core::Stream;

use crate::cli::DevPanelArgs;

pub(super) struct Scratch {
    pub(super) path: PathBuf,
    remove_on_drop: bool,
}

impl Scratch {
    pub(super) fn new(kind: &str) -> Result<Self> {
        let base = runtime_dir()
            .context("XDG_RUNTIME_DIR must name an absolute directory")?
            .join(kind);
        fs::create_dir_all(&base)?;
        let path = base.join(glib::uuid_string_random().as_str());
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self {
            path,
            remove_on_drop: true,
        })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if self.remove_on_drop {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

pub(super) struct OwnedPanel {
    child: Child,
    bus: zbus::Connection,
    pub(super) app_id: String,
    scratch: Scratch,
}

impl OwnedPanel {
    pub(super) async fn start(
        args: DevPanelArgs,
        config: Option<&Path>,
        bus: &zbus::Connection,
    ) -> Result<Self> {
        let executable = std::env::current_exe()?.with_file_name("glimpse-panel");
        Self::start_with(args, config, bus, &executable).await
    }

    async fn start_with(
        args: DevPanelArgs,
        config: Option<&Path>,
        bus: &zbus::Connection,
        executable: &Path,
    ) -> Result<Self> {
        let mut interrupt = signal(SignalKind::interrupt())?;
        let mut terminate = signal(SignalKind::terminate())?;
        let mut hangup = signal(SignalKind::hangup())?;
        let app_id = args.app_id.unwrap_or_else(|| {
            format!(
                "me.aresa.GlimpsePanel.Dev.p{}",
                glib::uuid_string_random().replace('-', "")
            )
        });
        ensure!(
            zbus::names::WellKnownName::try_from(app_id.as_str()).is_ok(),
            "invalid panel app id"
        );
        ensure!(
            !super::has_owner(bus, &app_id).await?
                && !super::has_owner(bus, &applet_host_name(&app_id)).await?,
            "panel app id is already owned: {app_id}"
        );
        let default_config = config.is_none();
        let position = match args.edge {
            crate::cli::Position::Top => glimpse_config::Position::Top,
            crate::cli::Position::Bottom => glimpse_config::Position::Bottom,
            crate::cli::Position::Left => glimpse_config::Position::Left,
            crate::cli::Position::Right => glimpse_config::Position::Right,
        };
        let mut config: Config = match config {
            Some(path) => toml::from_str(
                &fs::read_to_string(path)
                    .with_context(|| format!("cannot read {}", path.display()))?,
            )?,
            None => Config::default(),
        };
        if default_config
            || config.panels.is_empty()
            || args.output.is_some()
            || !args.applets.is_empty()
        {
            config.panels = vec![Panel {
                monitor: args.output,
                position,
                left: Vec::new(),
                center: args.applets,
                right: Vec::new(),
                ..Panel::default()
            }];
        } else {
            config.panels[0].position = position;
        }
        ensure!(
            config.validate().is_empty(),
            "invalid development panel config: {:?}",
            config.validate()
        );
        let mut scratch = Scratch::new("panels")?;
        let path = scratch.path.join("config.toml");
        fs::write(&path, toml::to_string_pretty(&config)?)?;
        let child = Command::new(executable)
            .arg("--config")
            .arg(&path)
            .env("GLIMPSE_PANEL_APP_ID", &app_id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .process_group(0)
            .spawn()
            .with_context(|| format!("cannot start {}", executable.display()))?;
        scratch.remove_on_drop = false;
        let mut panel = Self {
            child,
            bus: bus.clone(),
            app_id,
            scratch,
        };
        let ready = async {
            tokio::time::timeout(crate::errors::DEADLINE, async {
                loop {
                    if let Some(status) = panel.child.try_wait()? {
                        anyhow::bail!("development panel exited during startup: {status}");
                    }
                    let name = applet_host_name(&panel.app_id);
                    if super::has_owner(bus, &name).await? {
                        let dbus = super::within(zbus::fdo::DBusProxy::new(bus)).await?;
                        let pid = super::within(
                            dbus.get_connection_unix_process_id(name.as_str().try_into()?),
                        )
                        .await?;
                        ensure!(
                            pid == panel.child.id(),
                            "development panel name is owned by another process"
                        );
                        return Ok(());
                    }
                    tokio::time::sleep(Duration::from_millis(30)).await;
                }
            })
            .await
            .context("development panel startup timed out")?
        };
        let result = tokio::select! {
            result = ready => result,
            _ = interrupt.recv() => Err(anyhow::anyhow!("development panel startup interrupted")),
            _ = terminate.recv() => Err(anyhow::anyhow!("development panel startup interrupted")),
            _ = hangup.recv() => Err(anyhow::anyhow!("development panel startup interrupted")),
        };
        if let Err(error) = result {
            if let Err(cleanup) = panel.shutdown().await {
                eprintln!("cannot clean up development panel applets: {cleanup:#}");
            }
            return Err(error);
        }
        Ok(panel)
    }

    pub(super) fn exited(&mut self) -> Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }

    pub(super) async fn shutdown(&mut self) -> Result<()> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let result = self.stop_units().await;
        self.scratch.remove_on_drop = result.is_ok();
        result
    }

    async fn stop_units(&self) -> Result<()> {
        tokio::time::timeout(crate::errors::DEADLINE, self.finish_units())
            .await
            .map_err(|_| {
                anyhow::Error::new(crate::errors::TimedOut)
                    .context("development panel applet cleanup timed out")
            })?
    }

    async fn finish_units(&self) -> Result<()> {
        if !super::has_owner(&self.bus, "org.freedesktop.systemd1").await? {
            return Ok(());
        }
        let manager =
            super::within(glimpse_dbus::systemd1::Systemd1ManagerProxy::new(&self.bus)).await?;
        manager.subscribe().await?;
        let mut removed = Box::pin(manager.receive_job_removed().await?);
        let prefix = format!("app-glimpse-{}-", self.app_id.replace('-', "\\x2d"));
        let pattern = format!("{prefix}*.service");
        let units = super::within(manager.list_units_by_patterns(&[], &[pattern.as_str()])).await?;
        let mut failed = None;
        for entry in units {
            if !owned_unit(&entry.0, &prefix, self.child.id()) {
                continue;
            }
            let stop = manager.stop_unit(&entry.0, "replace");
            tokio::pin!(stop);
            let mut job = None;
            let mut completed = std::collections::HashMap::new();
            loop {
                tokio::select! {
                    result = &mut stop, if job.is_none() => {
                        match result {
                            Ok(path) => job = Some(path),
                            Err(error) => {
                                eprintln!("cannot stop {}: {error:#}", entry.0);
                                failed.get_or_insert(anyhow::Error::from(error));
                                break;
                            }
                        }
                    }
                    signal = std::future::poll_fn(|cx| removed.as_mut().poll_next(cx)) => {
                        let signal = signal.context("systemd stopped reporting applet stop jobs")?;
                        let args = signal.args()?;
                        if args.unit == entry.0 {
                            completed.insert(args.job.clone(), args.result.to_owned());
                        }
                    }
                }
                if let Some(result) = job.as_ref().and_then(|job| completed.remove(job)) {
                    if result != "done" {
                        failed.get_or_insert_with(|| {
                            anyhow::anyhow!("stopping {} ended with {result}", entry.0)
                        });
                    }
                    break;
                }
            }
        }
        failed.map_or(Ok(()), Err)
    }
}

fn owned_unit(name: &str, prefix: &str, pid: u32) -> bool {
    let Some(body) = name
        .strip_prefix(prefix)
        .and_then(|body| body.strip_suffix(".service"))
    else {
        return false;
    };
    let parts: Vec<_> = body.split('-').collect();
    let [applet, slot, owner, count] = parts.as_slice() else {
        return false;
    };
    let number = |value: &str| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit());
    !applet.is_empty() && number(slot) && *owner == pid.to_string() && number(count)
}

impl Drop for OwnedPanel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub async fn dev_panel(args: DevPanelArgs, config: Option<PathBuf>, json: bool) -> Result<()> {
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    let bus = super::within(zbus::Connection::session()).await?;
    let mut panel = OwnedPanel::start(args, config.as_deref(), &bus).await?;
    let result = async {
        if json {
            super::emit(&serde_json::json!({"app_id": panel.app_id}))?;
        } else {
            println!("{}", panel.app_id);
        }
        loop {
            tokio::select! {
                _ = interrupt.recv() => break,
                _ = terminate.recv() => break,
                _ = hangup.recv() => break,
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    ensure!(!panel.exited()?, "development panel exited");
                }
            }
        }
        Ok(())
    }
    .await;
    if let Err(cleanup) = panel.shutdown().await {
        if result.is_ok() {
            return Err(cleanup);
        }
        eprintln!("cannot clean up development panel applets: {cleanup:#}");
    }
    result
}

#[cfg(test)]
mod tests;
