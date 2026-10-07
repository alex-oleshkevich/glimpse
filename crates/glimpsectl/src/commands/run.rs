use std::collections::VecDeque;
use std::fs;
use std::io::{self, IsTerminal};
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use futures_util::{SinkExt, StreamExt};
use glimpse_config::{AppletSandbox, ExecAppletConfig, is_desktop_id};
use glimpse_dbus::{applet_host::AppletHostProxy, applet_host_name, panel_app_id};
use glimpse_services::{
    DesktopCatalog, EXEC_HELLO_WINDOW, EXEC_MAX_HELLOS, ExecEntry, FromApplet, MAX_LINE, Outgoing,
    Tree, exec_confine, exec_sandbox, exec_state_dir,
};
use notify::{EventKind, RecursiveMode, Watcher};
use tokio::io::AsyncWriteExt;
use tokio::net::unix::pipe::{Receiver, Sender};
use tokio::process::{Child, ChildStdin, Command};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use tokio_util::sync::CancellationToken;
use zbus::fdo::DBusProxy;
use zbus::zvariant::Fd;

use crate::cli::{RunArgs, Zone};

type Lines = FramedRead<Receiver, LinesCodec>;

struct Link {
    input: Option<Lines>,
    output: Option<FramedWrite<Sender, LinesCodec>>,
    handle: String,
}

struct Applet {
    child: Child,
    input: FramedRead<tokio::process::ChildStdout, LinesCodec>,
    output: ChildStdin,
}

struct HostTarget<'a> {
    name: &'a str,
    panel: u32,
    output: &'a str,
    zone: &'a str,
}

fn source(dir: &Path) -> Result<(String, ExecEntry)> {
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

fn settings(path: &Path, id: &str) -> Result<ExecAppletConfig> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let dir = path.parent().context("applet config has no directory")?;
    let mut table: toml::Table =
        toml::from_str(&raw).with_context(|| format!("cannot parse {}", path.display()))?;
    table
        .entry("applet")
        .or_insert_with(|| toml::Value::String(id.to_owned()));
    let mut config: ExecAppletConfig = table
        .try_into()
        .with_context(|| format!("cannot parse {}", path.display()))?;
    ensure!(
        config.applet == id,
        "{} configures {}, not {id}",
        path.display(),
        config.applet
    );
    config.sandbox = config.sandbox.resolve(dir).map_err(anyhow::Error::msg)?;
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

fn granted(settings: &ExecAppletConfig, source: Option<&Path>) -> AppletSandbox {
    let mut write = settings.sandbox.write.clone();
    write.extend(source.map(|source| source.display().to_string()));
    AppletSandbox {
        mpris: settings.sandbox.mpris,
        compositor: settings.sandbox.compositor,
        launch: settings.sandbox.launch.clone(),
        write,
        ..AppletSandbox::default()
    }
}

async fn attach(
    proxy: &AppletHostProxy<'_>,
    target: &HostTarget<'_>,
    entry: &ExecEntry,
    settings: &ExecAppletConfig,
    source: Option<&Path>,
) -> Result<Link> {
    let (from_panel, to_applet) = io::pipe()?;
    let (from_applet, to_panel) = io::pipe()?;
    let handle = super::within(proxy.attach(
        entry.path.to_str().context("desktop path is not UTF-8")?,
        target.panel,
        target.output,
        target.zone,
        &serde_json::to_string(&settings.options)?,
        &serde_json::to_string(&granted(settings, source))?,
        Fd::from(&to_applet),
        Fd::from(&from_applet),
    ))
    .await
    .with_context(|| format!("cannot attach to the panel at {}", target.name))?;
    drop(to_applet);
    drop(from_applet);
    let link = Link {
        input: Some(FramedRead::new(
            Receiver::from_owned_fd(OwnedFd::from(from_panel))?,
            LinesCodec::new_with_max_length(MAX_LINE),
        )),
        output: Some(FramedWrite::new(
            Sender::from_owned_fd(OwnedFd::from(to_panel))?,
            LinesCodec::new_with_max_length(MAX_LINE),
        )),
        handle,
    };
    eprintln!(
        "attached to {} as {} (panel {}, output {}, zone {})",
        target.name,
        link.handle,
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
    Ok(link)
}

fn spawn(
    entry: &ExecEntry,
    id: &str,
    state: &Path,
    settings: &ExecAppletConfig,
    env: &[(String, String)],
    no_sandbox: bool,
    source: Option<&Path>,
) -> Result<Applet> {
    let (program, args) = entry.argv.split_first().context("empty Exec")?;
    let ruleset = if no_sandbox {
        None
    } else {
        let mut sandbox = settings.sandbox.clone();
        if let Some(source) = source {
            let source = source
                .to_str()
                .context("applet directory path is not UTF-8")?;
            sandbox.read.push(source.to_owned());
            sandbox.write.push(source.to_owned());
        }
        exec_sandbox(&sandbox, id, state).map_err(anyhow::Error::msg)?
    };
    let parent = rustix::process::getpid();
    let ruleset_fd = ruleset.as_ref().map(AsRawFd::as_raw_fd);
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .process_group(0)
        .kill_on_drop(true);
    if let Some(cwd) = &entry.cwd {
        command.current_dir(cwd);
    }
    for (key, value) in env {
        command.env(key, value);
    }
    unsafe {
        command.pre_exec(move || exec_confine(ruleset_fd, parent));
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("cannot start {program}"))?;
    eprintln!(
        "applet started (pid {})",
        child.id().context("spawned applet has no pid")?
    );
    drop(ruleset);
    let input = FramedRead::new(
        child.stdout.take().context("no applet stdout")?,
        LinesCodec::new_with_max_length(MAX_LINE),
    );
    let output = child.stdin.take().context("no applet stdin")?;
    Ok(Applet {
        child,
        input,
        output,
    })
}

async fn stop(applet: &mut Option<Applet>, natural_exit: bool) -> Result<Option<ExitStatus>> {
    let Some(mut old) = applet.take() else {
        return Ok(None);
    };
    if let Some(group) = old
        .child
        .id()
        .and_then(|pid| rustix::process::Pid::from_raw(pid as i32))
    {
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
    let status = old.child.wait().await?;
    Ok((natural_exit && status.code().is_some()).then_some(status))
}

fn applet_failure(watch: bool, reason: impl std::fmt::Display) -> Result<()> {
    if watch {
        eprintln!("{reason}; waiting for a change");
        Ok(())
    } else {
        anyhow::bail!("{reason}")
    }
}

fn fatal_reattach(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        let Some(error) = cause.downcast_ref::<zbus::Error>() else {
            return false;
        };
        match error {
            zbus::Error::MethodError(name, message, _) => match name.as_str() {
                "org.freedesktop.DBus.Error.InvalidArgs"
                | "org.freedesktop.DBus.Error.AccessDenied" => true,
                "org.freedesktop.DBus.Error.Failed" => message
                    .as_deref()
                    .is_some_and(|message| message.contains("already attached")),
                _ => false,
            },
            zbus::Error::FDO(error) => match error.as_ref() {
                zbus::fdo::Error::InvalidArgs(_) | zbus::fdo::Error::AccessDenied(_) => true,
                zbus::fdo::Error::Failed(message) => message.contains("already attached"),
                _ => false,
            },
            _ => false,
        }
    })
}

async fn send_to_panel(link: &mut Link, line: String, trace: bool) -> Result<()> {
    if trace {
        eprintln!("→ {line}");
    }
    link.output
        .as_mut()
        .context("panel pipe closed")?
        .send(line)
        .await?;
    Ok(())
}

async fn wait_for_owner(dbus: &DBusProxy<'_>, name: &str) -> Result<()> {
    let mut changes = dbus
        .receive_name_owner_changed_with_args(&[(0, name)])
        .await?;
    if super::within(dbus.name_has_owner(name.try_into()?)).await? {
        return Ok(());
    }
    while let Some(change) = changes.next().await {
        if change.args()?.new_owner.is_some() {
            return Ok(());
        }
    }
    anyhow::bail!("session bus disconnected while waiting for panel")
}

pub async fn applets_run(args: RunArgs) -> Result<()> {
    let RunArgs {
        target,
        dev: watch,
        panel,
        output,
        zone,
        env: environment,
        trace,
        no_sandbox,
        applet_config,
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
            let path = dir.join("config.toml");
            ensure!(
                path.is_file(),
                "no config.toml in {}; create one or pass --applet-config <file>",
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
    let state = exec_state_dir(
        &glimpse_config::state_dir().context("no state directory")?,
        &id,
    )
    .map_err(anyhow::Error::msg)?;
    let environment = environment
        .into_iter()
        .map(|item| {
            let (key, value) = item.split_once('=').context("--env expects KEY=VALUE")?;
            ensure!(!key.is_empty(), "--env key is empty");
            Ok((key.to_owned(), value.to_owned()))
        })
        .collect::<Result<Vec<_>>>()?;
    let zone = zone.map_or("", Zone::as_str);
    let name = applet_host_name(&panel_app_id());
    let host = HostTarget {
        name: &name,
        panel,
        output: &output,
        zone,
    };
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    let shutdown = CancellationToken::new();
    let shutdown_signal = shutdown.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = interrupt.recv() => {},
            _ = terminate.recv() => {},
            _ = hangup.recv() => {},
        }
        shutdown_signal.cancel();
    });
    let bus = zbus::Connection::session()
        .await
        .context("cannot reach the session bus")?;
    let dbus = DBusProxy::new(&bus).await?;
    let proxy = AppletHostProxy::builder(&bus)
        .destination(name.clone())?
        .build()
        .await?;
    let mut link = tokio::select! {
        result = attach(&proxy, &host, &entry, &current_settings, directory_target.then_some(dir.as_path())) => result?,
        _ = shutdown.cancelled() => return Ok(()),
    };
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
    let mut starts = VecDeque::new();
    let mut applet = Some(spawn(
        &entry,
        &id,
        &state,
        &current_settings,
        &environment,
        no_sandbox,
        directory_target.then_some(dir.as_path()),
    )?);
    starts.push_back(Instant::now());
    if !watch {
        eprintln!("not watching files; pass --dev to restart on change");
    }
    let mut tree = Tree::default();
    let mut pending_hellos = 0usize;
    let mut current_hello = false;
    let mut ready = false;
    let mut restart_at: Option<Instant> = None;
    let mut restart_reason: Option<String> = None;
    let mut changed_files: Vec<PathBuf> = Vec::new();
    let mut deferred = false;
    let mut reattach = false;
    println!("Running {id}. Press Ctrl-C to stop.");

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            command = commands.recv(), if tty => {
                match command {
                    Some(line) if line.trim() == "r" => restart_at = Some(Instant::now()),
                    None => tty = false,
                    _ => {}
                }
            }
            event = events_rx.recv(), if watch => {
                if let Some(Ok(event)) = event {
                    let mut changed = false;
                    let mut config_changed = false;
                    for path in event.paths {
                        if applet_config == path { config_changed = true; }
                        else if if directory_target { watched(&path, &dir) } else { path == entry.path } {
                            changed = true;
                            changed_files.push(path);
                        }
                    }
                    if config_changed {
                        match settings(&applet_config, &id) {
                            Ok(next) => {
                                if next.options != current_settings.options {
                                    match super::within(proxy.set_options(&link.handle, &serde_json::to_string(&next.options)?)).await {
                                        Ok(()) => eprintln!("options updated"),
                                        Err(error) => eprintln!("cannot update options: {error}"),
                                    }
                                }
                                let source = directory_target.then_some(dir.as_path());
                                if granted(&next, source) != granted(&current_settings, source) {
                                    anyhow::bail!("service, launch or write grants changed; run again to apply them");
                                }
                                if next.sandbox != current_settings.sandbox {
                                    changed = true;
                                    restart_reason = Some("sandbox changed, restarting".to_owned());
                                }
                                current_settings = next;
                            }
                            Err(error) => eprintln!("{error:#}"),
                        }
                    }
                    if changed {
                        let settled = Instant::now() + Duration::from_millis(300);
                        restart_at = Some(restart_at.map_or(settled, |at| at.max(settled)));
                    }
                }
            }
            _ = async { if let Some(at) = restart_at { tokio::time::sleep_until(at).await } else { std::future::pending().await } } => {
                let now = Instant::now();
                while starts.front().is_some_and(|at| now.duration_since(*at) >= EXEC_HELLO_WINDOW) { starts.pop_front(); }
                if starts.len() >= EXEC_MAX_HELLOS {
                    let deadline = *starts.front().context("restart window is empty")? + EXEC_HELLO_WINDOW;
                    restart_at = Some(deadline);
                    if !deferred {
                        eprintln!("reload deferred {}s", deadline.duration_since(now).as_secs());
                        deferred = true;
                    }
                } else {
                    if let Some(reason) = restart_reason.take().or_else(|| changed_file(&changed_files, &dir)) { eprintln!("{reason}"); }
                    changed_files.clear();
                    stop(&mut applet, false).await?;
                    starts.push_back(now);
                    if reattach {
                        match attach(&proxy, &host, &entry, &current_settings, directory_target.then_some(dir.as_path())).await {
                            Ok(next) => {
                                link = next;
                                eprintln!("re-attached as {}", link.handle);
                                reattach = false;
                                pending_hellos = 0;
                            }
                            Err(error) => {
                                if fatal_reattach(&error) { return Err(error); }
                                eprintln!("cannot attach applet: {error:#}");
                                if !super::within(dbus.name_has_owner(host.name.try_into()?)).await.unwrap_or(false) {
                                    eprintln!("panel went away, waiting for it");
                                    tokio::select! {
                                        result = wait_for_owner(&dbus, host.name) => result?,
                                        _ = shutdown.cancelled() => break,
                                    }
                                }
                                restart_at = Some(Instant::now() + Duration::from_millis(300));
                                continue;
                            }
                        }
                    }
                    tree = Tree::default();
                    ready = false;
                    current_hello = false;
                    match DesktopCatalog.resolve_file(&entry.path) {
                        Ok(next) => {
                            if next.name != entry.name || next.icon != entry.icon {
                                eprintln!("desktop name or icon changed; the panel shows it after the next `applets run`");
                            }
                            entry = next;
                        }
                        Err(error) => eprintln!("cannot reload desktop entry: {error}"),
                    }
                    applet = match spawn(&entry, &id, &state, &current_settings, &environment, no_sandbox, directory_target.then_some(dir.as_path())) {
                        Ok(next) => Some(next),
                        Err(error) => { applet_failure(watch, &error)?; None },
                    };
                    restart_at = None;
                    deferred = false;
                }
            }
            line = async { if let Some(applet) = applet.as_mut() { applet.input.next().await } else { std::future::pending().await } } => {
                match line {
                    Some(Ok(line)) => {
                        let parsed = serde_json::from_str::<FromApplet>(&line);
                        if trace { eprintln!("← applet {line}"); }
                        match parsed {
                            Ok(FromApplet::Hello { v: 1 }) => {
                                if !current_hello { eprintln!("applet ready"); }
                                tree = Tree::default();
                                pending_hellos += 1;
                                ready = false;
                                current_hello = true;
                                if send_to_panel(&mut link, line, trace).await.is_err() {
                                    if !panel_lost(&dbus, &mut link, &mut applet, &host, &shutdown).await? { break; }
                                    reattach = true;
                                    restart_at = Some(Instant::now());
                                }
                            }
                            Ok(FromApplet::Hello { v }) => {
                                stop(&mut applet, false).await?;
                                ready = false;
                                current_hello = false;
                                applet_failure(watch, format!("unsupported applet protocol version {v}"))?;
                            }
                            Ok(FromApplet::Commit { ops }) => match tree.apply(ops) {
                                Ok(next) => {
                                    tree = next;
                                    if send_to_panel(&mut link, line, trace).await.is_err() {
                                        if !panel_lost(&dbus, &mut link, &mut applet, &host, &shutdown).await? { break; }
                                        reattach = true;
                                        restart_at = Some(Instant::now());
                                    }
                                }
                                Err(error) => { stop(&mut applet, false).await?; ready = false; current_hello = false; applet_failure(watch, format!("commit rejected: {error}"))?; }
                            },
                            Ok(_) => {
                                if send_to_panel(&mut link, line, trace).await.is_err() {
                                    if !panel_lost(&dbus, &mut link, &mut applet, &host, &shutdown).await? { break; }
                                    reattach = true;
                                    restart_at = Some(Instant::now());
                                }
                            }
                            Err(error) => { stop(&mut applet, false).await?; ready = false; current_hello = false; applet_failure(watch, format!("invalid applet message: {error}"))?; }
                        }
                    }
                    Some(Err(error)) => { stop(&mut applet, false).await?; ready = false; current_hello = false; applet_failure(watch, format!("invalid applet line: {error}"))?; }
                    None => {
                        let status = stop(&mut applet, true).await?;
                        ready = false;
                        current_hello = false;
                        let status = status.map_or_else(|| "unknown status".to_owned(), |status| status.to_string());
                        applet_failure(watch, format!("applet exited ({status})"))?;
                    }
                }
            }
            line = async { if let Some(input) = link.input.as_mut() { input.next().await } else { std::future::pending().await } } => {
                match line {
                    Some(Ok(line)) => {
                        if trace { eprintln!("← panel {line}"); }
                        let hello = matches!(serde_json::from_str::<Outgoing>(&line), Ok(Outgoing::Hello { .. }));
                        let forward = if hello && pending_hellos > 0 {
                            pending_hellos -= 1;
                            let last = pending_hellos == 0 && current_hello;
                            if last { ready = true; }
                            last
                        } else { !hello && ready };
                        if forward
                            && let Some(current) = applet.as_mut()
                            && (current.output.write_all(line.as_bytes()).await.is_err() || current.output.write_all(b"\n").await.is_err()) {
                            stop(&mut applet, false).await?;
                            ready = false;
                            applet_failure(watch, "cannot send to applet")?;
                        } else if forward && trace {
                            eprintln!("→ applet {line}");
                        }
                    }
                    Some(Err(error)) => {
                        eprintln!("invalid panel line: {error}");
                        if !panel_lost(&dbus, &mut link, &mut applet, &host, &shutdown).await? { break; }
                        reattach = true;
                        restart_at = Some(Instant::now());
                    }
                    None => {
                        if !panel_lost(&dbus, &mut link, &mut applet, &host, &shutdown).await? { break; }
                        reattach = true;
                        restart_at = Some(Instant::now());
                    }
                }
            }
        }
    }
    stop(&mut applet, false).await?;
    Ok(())
}

async fn panel_lost(
    dbus: &DBusProxy<'_>,
    link: &mut Link,
    applet: &mut Option<Applet>,
    target: &HostTarget<'_>,
    shutdown: &CancellationToken,
) -> Result<bool> {
    stop(applet, false).await?;
    link.input.take();
    tokio::select! {
        result = async {
            let owned = super::within(dbus.name_has_owner(target.name.try_into()?)).await?;
            link.output.take();
            if owned {
                eprintln!("panel dropped the applet, re-attaching");
            } else {
                eprintln!("panel went away, waiting for it");
                wait_for_owner(dbus, target.name).await?;
            }
            Ok::<_, anyhow::Error>(())
        } => { result?; Ok(true) },
        _ = shutdown.cancelled() => Ok(false),
    }
}

#[cfg(test)]
mod tests;
