use std::collections::HashMap;
use std::fs;
use std::io::{self, IsTerminal};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use glimpse_config::{ExecAppletConfig, exec_env_name_allowed, is_desktop_id};
use glimpse_dbus::{applet_host::AppletHostProxy, applet_host_name};
use glimpse_services::{DesktopCatalog, ExecEntry};
use notify::{EventKind, RecursiveMode, Watcher};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::Instant;
use zbus::zvariant::Fd;

use crate::cli::{RunArgs, Zone};

struct HostTarget<'a> {
    name: &'a str,
    panel: u32,
    output: &'a str,
    zone: &'a str,
}

pub(super) fn source(dir: &Path) -> Result<(String, ExecEntry)> {
    let mut desktops = fs::read_dir(dir)?
        .filter_map(|item| item.ok().map(|item| item.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "desktop"));
    let path = desktops
        .next()
        .context("no .desktop file in applet directory")?;
    ensure!(
        desktops.next().is_none(),
        "multiple .desktop files in applet directory"
    );
    let id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("invalid .desktop filename")?
        .to_owned();
    ensure!(is_desktop_id(&id), "invalid desktop-file id: {id}");
    let entry = DesktopCatalog
        .resolve_file(&path)
        .map_err(anyhow::Error::msg)?;
    Ok((id, entry))
}

pub(super) fn settings(path: &Path, id: &str) -> Result<ExecAppletConfig> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut table: toml::Table =
        toml::from_str(&raw).with_context(|| format!("cannot parse {}", path.display()))?;
    table
        .entry("applet")
        .or_insert_with(|| toml::Value::String(id.to_owned()));
    let config: ExecAppletConfig = table
        .try_into()
        .with_context(|| format!("cannot parse {}", path.display()))?;
    config
        .validate()
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("cannot parse {}", path.display()))?;
    ensure!(
        config.applet == id,
        "{} configures {}, not {id}",
        path.display(),
        config.applet
    );
    Ok(config)
}

fn changed_file(paths: &[PathBuf], dir: &Path) -> Option<String> {
    let path = paths
        .iter()
        .rev()
        .find(|path| path.exists())
        .or(paths.first())?;
    Some(format!(
        "restarting: {}",
        path.strip_prefix(dir).unwrap_or(path).display()
    ))
}

fn watched(path: &Path, dir: &Path) -> bool {
    path.strip_prefix(dir).is_ok_and(|relative| {
        relative.components().all(|part| {
            let name = part.as_os_str().to_string_lossy();
            !name.starts_with('.') && !matches!(name.as_ref(), "node_modules" | "build" | "dist")
        })
    })
}

async fn attach(
    proxy: &AppletHostProxy<'_>,
    target: &HostTarget<'_>,
    entry: &ExecEntry,
    config: &ExecAppletConfig,
    flags: &[(String, String)],
) -> Result<String> {
    let mut env: HashMap<&str, &str> = config
        .env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    env.extend(
        flags
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    let stderr = io::stderr().as_fd().try_clone_to_owned()?;
    let handle = super::within(proxy.attach(
        entry.path.to_str().context("desktop path is not UTF-8")?,
        target.panel,
        target.output,
        target.zone,
        &serde_json::to_string(&config.options)?,
        env,
        Fd::from(&stderr),
    ))
    .await
    .with_context(|| format!("cannot attach to the panel at {}", target.name))?;
    eprintln!(
        "attached to {} as {handle} (panel {}, output {}, zone {})",
        target.name,
        target.panel,
        if target.output.is_empty() {
            "first"
        } else {
            target.output
        },
        if target.zone.is_empty() {
            "default"
        } else {
            target.zone
        }
    );
    Ok(handle)
}

pub async fn applets_run(args: RunArgs, panel_app_id: &str, json: bool) -> Result<()> {
    let RunArgs {
        target,
        dev: watch,
        panel,
        output,
        zone,
        env: flags,
        applet_config,
        isolated,
    } = args;
    let directory_target = Path::new(&target).is_dir();
    let (id, mut entry, dir) = if directory_target {
        let dir = Path::new(&target).canonicalize()?;
        let (id, entry) = source(&dir)?;
        (id, entry, dir)
    } else {
        ensure!(
            is_desktop_id(&target),
            "{target} is neither an applet directory nor a desktop-file id"
        );
        let entry = DesktopCatalog
            .installed()
            .into_iter()
            .find_map(|(id, entry)| (id == target).then_some(entry))
            .with_context(|| format!("{target} is not installed"))?
            .map_err(anyhow::Error::msg)?;
        let dir = entry
            .path
            .parent()
            .context("installed desktop file has no directory")?
            .to_path_buf();
        (target, entry, dir)
    };
    let applet_config = match applet_config {
        Some(path) => path,
        None if directory_target => {
            let path = dir.join("devconfig.toml");
            ensure!(
                path.is_file(),
                "no devconfig.toml in {}; create one or pass --applet-config <file>",
                dir.display()
            );
            path
        }
        None => anyhow::bail!("{id} has no source directory; pass --applet-config <file>"),
    };
    let applet_config = applet_config
        .canonicalize()
        .with_context(|| format!("cannot read {}", applet_config.display()))?;
    let (events_tx, mut events_rx) = mpsc::unbounded_channel::<notify::Result<notify::Event>>();
    let _watcher = if watch {
        let watched_dir = dir.clone();
        let watched_file = entry.path.clone();
        let watched_config = applet_config.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event.as_ref().is_ok_and(|event| {
                    matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) && event.paths.iter().any(|path| {
                        watched_config == *path
                            || if directory_target {
                                watched(path, &watched_dir)
                            } else {
                                path == &watched_file
                            }
                    })
                }) {
                    let _ = events_tx.send(event);
                }
            })?;
        if directory_target {
            watcher.watch(&dir, RecursiveMode::Recursive)?;
        } else {
            watcher.watch(&dir, RecursiveMode::NonRecursive)?;
        }
        if let Some(parent) = applet_config.parent().filter(|parent| {
            if directory_target {
                !parent.starts_with(&dir)
            } else {
                *parent != dir.as_path()
            }
        }) {
            watcher.watch(parent, RecursiveMode::NonRecursive)?;
        }
        Some(watcher)
    } else {
        None
    };
    let mut current_settings = settings(&applet_config, &id)?;
    let flags = flags
        .into_iter()
        .map(|item| {
            let (key, value) = item.split_once('=').context("--env expects KEY=VALUE")?;
            ensure!(
                exec_env_name_allowed(key),
                "--env name is not allowed: {key:?}"
            );
            Ok((key.to_owned(), value.to_owned()))
        })
        .collect::<Result<Vec<_>>>()?;
    let zone = zone.map_or("", Zone::as_str);
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    let bus = super::within(zbus::Connection::session())
        .await
        .context("cannot reach the session bus")?;
    let mut isolated_panel = if isolated {
        Some(
            super::dev::OwnedPanel::start(
                crate::cli::DevPanelArgs {
                    app_id: None,
                    output: (!output.is_empty()).then(|| output.clone()),
                    edge: crate::cli::Position::Bottom,
                    applets: Vec::new(),
                },
                None,
                &bus,
            )
            .await?,
        )
    } else {
        None
    };
    let result = async {
    let name = applet_host_name(isolated_panel.as_ref().map_or(panel_app_id, |panel| panel.app_id.as_str()));
    let host = HostTarget {
        name: &name,
        panel,
        output: &output,
        zone,
    };
    ensure!(
        super::has_owner(&bus, &name).await?,
        "no panel is running: {name} is not on the session bus"
    );
    let proxy = AppletHostProxy::builder(&bus)
        .destination(name.clone())?
        .build()
        .await?;
    let mut handle = Some(attach(&proxy, &host, &entry, &current_settings, &flags).await?);
    let mut tty = io::stdin().is_terminal();
    let (command_tx, mut commands) = mpsc::unbounded_channel();
    if tty {
        std::thread::spawn(move || {
            let stdin = io::stdin();
            loop {
                let mut line = String::new();
                if stdin
                    .read_line(&mut line)
                    .ok()
                    .filter(|size| *size > 0)
                    .is_none()
                    || command_tx.send(line).is_err()
                {
                    break;
                }
            }
        });
    }
    if !watch {
        eprintln!("not watching files; pass --dev to restart on change");
    }
    let mut restart_at: Option<Instant> = None;
    let mut restart_reason: Option<String> = None;
    let mut config_at: Option<Instant> = None;
    let mut changed_files: Vec<PathBuf> = Vec::new();
    let mut reattach = false;
    eprintln!("Running {id}. Press Ctrl-C to stop; r restarts, t prints the current tree.");
    if json { super::emit(&serde_json::json!({"id": id, "handle": handle, "panel": isolated_panel.as_ref().map_or(panel_app_id, |panel| panel.app_id.as_str())}))?; }

    loop {
        tokio::select! {
            _ = interrupt.recv() => break,
            _ = terminate.recv() => break,
            _ = hangup.recv() => break,
            _ = tokio::time::sleep(Duration::from_millis(100)), if isolated_panel.is_some() => {
                if let Some(panel) = isolated_panel.as_mut() { ensure!(!panel.exited()?, "development panel exited"); }
            }
            command = commands.recv(), if tty => {
                match command {
                    Some(line) if line.trim() == "r" => restart_at = Some(Instant::now()),
                    Some(line) if line.trim() == "t" => {
                        if let Some(handle) = handle.as_deref() {
                            match super::within(proxy.tree(handle)).await {
                                Ok(tree) => { if let crate::render::Flow::Stop = crate::render::print(&tree)? { break; } },
                                Err(error) => eprintln!("cannot inspect tree: {error}"),
                            }
                        }
                    }
                    None => tty = false,
                    _ => {}
                }
            }
            event = events_rx.recv(), if watch => {
                if let Some(Ok(event)) = event {
                    let mut changed = false;
                    let settled = Instant::now() + Duration::from_millis(300);
                    for path in event.paths {
                        if applet_config == path { config_at = Some(config_at.map_or(settled, |at| at.max(settled))); }
                        else if if directory_target { watched(&path, &dir) } else { path == entry.path } {
                            changed = true;
                            reattach |= path == entry.path;
                            changed_files.push(path);
                        }
                    }
                    if changed {
                        restart_at = Some(restart_at.map_or(settled, |at| at.max(settled)));
                    }
                }
            }
            _ = async { if let Some(at) = config_at { tokio::time::sleep_until(at).await } else { std::future::pending().await } } => {
                config_at = None;
                match settings(&applet_config, &id) {
                    Ok(next) => {
                        if let Some(handle) = handle.as_deref().filter(|_| next.options != current_settings.options) {
                            match super::within(proxy.set_options(handle, &serde_json::to_string(&next.options)?)).await {
                                Ok(()) => eprintln!("options updated"),
                                Err(error) => eprintln!("cannot update options: {error}"),
                            }
                        }
                        if next.env != current_settings.env {
                            reattach = true;
                            restart_reason = Some("env changed, restarting".to_owned());
                            restart_at.get_or_insert_with(Instant::now);
                        }
                        current_settings = next;
                    }
                    Err(error) => eprintln!("{error:#}"),
                }
            }
            _ = async { if let Some(at) = restart_at { tokio::time::sleep_until(at).await } else { std::future::pending().await } } => {
                restart_at = None;
                if let Some(reason) = restart_reason.take().or_else(|| changed_file(&changed_files, &dir)) { eprintln!("{reason}"); }
                changed_files.clear();
                if reattach {
                    reattach = false;
                    match DesktopCatalog.resolve_file(&entry.path) {
                        Ok(next) => entry = next,
                        Err(error) => {
                            eprintln!("cannot reload desktop entry: {error}");
                            continue;
                        }
                    }
                } else if let Some(current) = handle.as_deref() {
                    match super::within(proxy.restart(current)).await {
                        Ok(()) => continue,
                        Err(error) => eprintln!("cannot restart {current}: {error}; attaching again"),
                    }
                }
                if let Some(handle) = handle.take()
                    && let Err(error) = super::within(proxy.detach(&handle)).await
                {
                    eprintln!("cannot detach {handle}: {error}");
                }
                match attach(&proxy, &host, &entry, &current_settings, &flags).await {
                    Ok(next) => handle = Some(next),
                    Err(error) => eprintln!("{error:#}; waiting for the next change"),
                }
            }
        }
    }
    if let Some(handle) = handle {
        super::within(proxy.detach(&handle))
            .await
            .with_context(|| format!("cannot detach {handle}"))?;
    }
        Ok(())
    }.await;
    if let Some(panel) = isolated_panel.as_mut()
        && let Err(cleanup) = panel.shutdown().await
    {
        if result.is_ok() {
            return Err(cleanup);
        }
        eprintln!("cannot clean up development panel applets: {cleanup:#}");
    }
    result
}

#[cfg(test)]
mod tests;
