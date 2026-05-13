//! Dev-mode supervisor: builds + watches + spawns the applet binary, and
//! proxies stdio between Glimpse and the applet. On source change, we
//! rebuild and respawn the applet; the cached `init` line is replayed to
//! the new child so it starts the same way Glimpse would have.

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::{Mutex, mpsc};

use crate::commands::config::glimpse_config_dir;
use crate::project::Language;

#[derive(ClapArgs)]
pub struct Args {
    /// Project directory (containing Cargo.toml / pyproject.toml /
    /// package.json / go.mod). Defaults to the current directory.
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
    /// Override language detection.
    #[arg(long, value_enum)]
    lang: Option<Language>,
    /// Debounce window for file-change events, in milliseconds. Multiple
    /// edits within this window are coalesced into a single rebuild.
    #[arg(long, default_value_t = 300)]
    debounce_ms: u64,
}

pub fn log(msg: impl AsRef<str>) {
    // Anything we say to the user goes to stderr — stdout is reserved
    // for the applet protocol stream that Glimpse parses.
    eprintln!("[glimpse-applet dev] {}", msg.as_ref());
}

pub async fn run(args: Args) -> Result<()> {
    let path = args.path.unwrap_or_else(|| PathBuf::from("."));
    let path = path
        .canonicalize()
        .with_context(|| format!("resolve {}", path.display()))?;
    let lang = match args.lang {
        Some(l) => l,
        None => Language::detect(&path)?.0,
    };
    let plan = build_plan(lang, &path);
    log(format!(
        "watching {} ({}) — sources: {}",
        path.display(),
        lang.name(),
        plan.watch_dirs
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));

    let applet_name = read_applet_id(&path).unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("dev-applet")
            .to_string()
    });

    // Only register the dev config when running standalone (stdin is a tty).
    // When glimpse-shell spawns us as a child, stdin is a pipe — that instance
    // must not register or clean up the config file.
    let _dev_config_guard = if std::io::stdin().is_terminal() {
        match install_dev_config(&applet_name, &plan) {
            Ok(path) => {
                check_dev_panel_config(&applet_name);
                Some(DevConfigGuard(path))
            }
            Err(e) => {
                log(format!("warning: could not write dev config: {e:#}"));
                None
            }
        }
    } else {
        None
    };

    // Cached init line. The supervisor reads it once from glimpse-shell at
    // startup and replays it to every child spawned afterward so a rebuild
    // doesn't lose the applet instance name + options.
    let init_line: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    // The currently running applet child (if any).
    let child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));

    // Channel: file watcher → supervisor (debounced "something changed").
    let (rebuild_tx, mut rebuild_rx) = mpsc::channel::<()>(8);

    // Watcher.
    let _watcher = start_watcher(&plan.watch_dirs, args.debounce_ms, rebuild_tx.clone())?;

    // Initial build + spawn.
    if let Err(e) = rebuild_and_respawn(&plan, child.clone(), init_line.clone()).await {
        log(format!("initial build failed: {e:#}"));
        // We continue anyway so the user can fix and retry. A subsequent
        // file change will trigger a rebuild.
    }

    // stdin proxy: read lines from parent stdin (Glimpse), cache `init`
    // lines, forward everything to the running child.
    let mut stdin_task = {
        let child = child.clone();
        let init_line = init_line.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(tokio::io::stdin());
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf).await {
                    Ok(0) => break, // EOF from Glimpse
                    Ok(_) => {}
                    Err(e) => {
                        log(format!("stdin read error: {e}"));
                        break;
                    }
                }
                // Cache init lines so we can replay them after a restart.
                if buf.starts_with("init ") {
                    *init_line.lock().await = Some(buf.clone());
                }
                forward_to_child(&child, buf.as_bytes()).await;
            }
            log("Glimpse stdin closed; exiting");
        })
    };

    // Forward SIGTERM into a channel so select! stays platform-neutral.
    let (term_tx, mut term_rx) = mpsc::channel::<()>(1);
    #[cfg(unix)]
    {
        if let Ok(mut sigterm) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::spawn(async move {
                if sigterm.recv().await.is_some() {
                    let _ = term_tx.send(()).await;
                }
            });
        }
    }

    // Supervisor loop: handle file change events, Ctrl-C, or SIGTERM.
    loop {
        tokio::select! {
            biased;
            _ = &mut stdin_task => break,
            _ = tokio::signal::ctrl_c() => break,
            _ = term_rx.recv() => break,
            Some(()) = rebuild_rx.recv() => {
                // Drain any further bursts.
                while rebuild_rx.try_recv().is_ok() {}
                if let Err(e) = rebuild_and_respawn(&plan, child.clone(), init_line.clone()).await {
                    log(format!("rebuild failed: {e:#}"));
                }
            }
        }
    }

    // Kill running child. The dev config is removed by _dev_config_guard's Drop.
    {
        let mut guard = child.lock().await;
        if let Some(mut c) = guard.take() {
            let _ = c.start_kill();
            let _ = c.wait().await;
        }
    }

    Ok(())
}

struct DevConfigGuard(PathBuf);

impl Drop for DevConfigGuard {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.0) {
            eprintln!(
                "[glimpse-applet dev] warning: could not remove dev config {}: {e}",
                self.0.display()
            );
        } else {
            eprintln!(
                "[glimpse-applet dev] removed dev config {}",
                self.0.display()
            );
        }
    }
}

fn read_applet_id(project_dir: &Path) -> Option<String> {
    let toml_path = project_dir.join("applet.toml");
    let content = std::fs::read_to_string(&toml_path).ok()?;
    let value: toml::Value = toml::from_str(&content).ok()?;
    let id = value.get("id")?.as_str()?.to_string();
    if id.is_empty() { None } else { Some(id) }
}

fn install_dev_config(name: &str, plan: &Plan) -> Result<PathBuf> {
    let applets_dir = glimpse_config_dir().join("applets");
    std::fs::create_dir_all(&applets_dir)
        .with_context(|| format!("create applets dir {}", applets_dir.display()))?;

    let exe = std::env::current_exe().context("resolve current executable")?;
    let mut table = toml::map::Map::new();
    table.insert(
        "command".into(),
        toml::Value::Array(vec![
            toml::Value::String(exe.display().to_string()),
            toml::Value::String("dev".into()),
            toml::Value::String(plan.workdir.display().to_string()),
        ]),
    );
    table.insert(
        "work_dir".into(),
        toml::Value::String(plan.workdir.display().to_string()),
    );
    let body = toml::to_string(&toml::Value::Table(table)).context("serialize dev config")?;
    let content = format!("# Generated by glimpse-applet dev — do not edit manually\n{body}");

    let config_path = applets_dir.join(format!("{name}.dev.toml"));
    std::fs::write(&config_path, content)
        .with_context(|| format!("write dev config {}", config_path.display()))?;
    log(format!(
        "registered dev config at {}",
        config_path.display()
    ));
    Ok(config_path)
}

fn check_dev_panel_config(applet_name: &str) {
    let config_file = glimpse_config_dir().join("config.toml");
    let content = match std::fs::read_to_string(&config_file) {
        Ok(c) => c,
        Err(_) => return,
    };
    let value: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let panels = match value.get("panels").and_then(|v| v.as_array()) {
        Some(p) => p,
        None => {
            log(format!(
                "warning: add the \"dev\" applet slot to a panel's left/center/right in {} to see {applet_name} in the bar",
                config_file.display()
            ));
            return;
        }
    };

    let has_dev = panels.iter().any(|panel| {
        ["left", "center", "right"].iter().any(|section| {
            panel
                .get(section)
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().any(|item| item.as_str() == Some("dev")))
                .unwrap_or(false)
        })
    });

    if !has_dev {
        log(format!(
            "warning: the \"dev\" applet slot is not in any panel's left/center/right in {} — add it to see {applet_name} in the bar",
            config_file.display()
        ));
    }
}

async fn forward_to_child(child: &Arc<Mutex<Option<Child>>>, bytes: &[u8]) {
    let mut guard = child.lock().await;
    if let Some(ref mut c) = guard.as_mut() {
        if let Some(stdin) = c.stdin.as_mut() {
            if let Err(e) = stdin.write_all(bytes).await {
                log(format!("forward to child failed: {e}"));
            }
        }
    }
}

struct Plan {
    binary: String,
    args: Vec<String>,
    workdir: PathBuf,
    /// How to build before the first spawn and after every change.
    build: Option<(String, Vec<String>)>,
    /// Source roots to watch (recursive).
    watch_dirs: Vec<PathBuf>,
}

fn build_plan(lang: Language, path: &Path) -> Plan {
    match lang {
        Language::Rust => Plan {
            binary: "cargo".into(),
            args: vec!["run".into(), "--quiet".into()],
            workdir: path.to_path_buf(),
            // `cargo run` builds-as-needed, so the explicit build is
            // redundant; but doing it separately gives us a faster failure
            // signal when there's a compile error.
            build: Some(("cargo".into(), vec!["build".into(), "--quiet".into()])),
            watch_dirs: vec![path.join("src"), path.join("Cargo.toml")],
        },
        Language::Python => Plan {
            binary: "python".into(),
            args: vec!["main.py".into()],
            workdir: path.to_path_buf(),
            build: None,
            watch_dirs: vec![
                path.to_path_buf(),
                path.join("main.py"),
                path.join("pyproject.toml"),
            ],
        },
        Language::Typescript => Plan {
            binary: "node".into(),
            args: vec!["dist/main.js".into()],
            workdir: path.to_path_buf(),
            build: Some(("npx".into(), vec!["tsc".into()])),
            watch_dirs: vec![path.join("src"), path.join("tsconfig.json")],
        },
        Language::Go => {
            let bin = path.join(".dev-build");
            Plan {
                binary: bin.display().to_string(),
                args: vec![],
                workdir: path.to_path_buf(),
                build: Some((
                    "go".into(),
                    vec!["build".into(), "-o".into(), bin.display().to_string()],
                )),
                watch_dirs: vec![path.to_path_buf()],
            }
        }
    }
}

fn start_watcher(
    dirs: &[PathBuf],
    debounce_ms: u64,
    tx: mpsc::Sender<()>,
) -> Result<
    notify_debouncer_full::Debouncer<
        notify::RecommendedWatcher,
        notify_debouncer_full::RecommendedCache,
    >,
> {
    let mut debouncer = new_debouncer(
        Duration::from_millis(debounce_ms),
        None,
        move |result: notify_debouncer_full::DebounceEventResult| {
            if let Ok(events) = result {
                let interesting = events.iter().any(|e| {
                    !e.paths.iter().any(|p| {
                        let s = p.to_string_lossy();
                        s.contains("/target/")
                            || s.contains("/dist/")
                            || s.contains("/node_modules/")
                            || s.contains("/.git/")
                            || s.contains("__pycache__")
                            || s.ends_with(".dev-build")
                    })
                });
                if interesting {
                    let _ = tx.try_send(());
                }
            }
        },
    )
    .context("create file watcher")?;

    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        let mode = if dir.is_dir() {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        debouncer
            .watch(dir, mode)
            .with_context(|| format!("watch {}", dir.display()))?;
    }
    Ok(debouncer)
}

async fn rebuild_and_respawn(
    plan: &Plan,
    child: Arc<Mutex<Option<Child>>>,
    init_line: Arc<Mutex<Option<String>>>,
) -> Result<()> {
    if let Some((cmd, args)) = &plan.build {
        log(format!("building: {cmd} {}", args.join(" ")));
        let status = TokioCommand::new(cmd)
            .args(args)
            .current_dir(&plan.workdir)
            .status()
            .await
            .with_context(|| format!("invoke {cmd}"))?;
        if !status.success() {
            bail!("build exited with {status}");
        }
    }

    // Kill old child, if any.
    {
        let mut guard = child.lock().await;
        if let Some(mut c) = guard.take() {
            let _ = c.start_kill();
            let _ = c.wait().await;
        }
    }

    // Spawn fresh child.
    log(format!("starting: {} {}", plan.binary, plan.args.join(" ")));
    let mut cmd = TokioCommand::new(&plan.binary);
    cmd.args(&plan.args)
        .current_dir(&plan.workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut new_child = cmd
        .spawn()
        .with_context(|| format!("spawn {}", plan.binary))?;

    // Replay the cached init line, if we have one.
    if let Some(ref init) = *init_line.lock().await {
        if let Some(stdin) = new_child.stdin.as_mut() {
            stdin
                .write_all(init.as_bytes())
                .await
                .context("replay init to child")?;
        }
    }

    // Pump child stdout to our stdout so Glimpse sees `status` /
    // `popover` messages.
    if let Some(stdout) = new_child.stdout.take() {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf).await {
                    Ok(0) => return,
                    Ok(_) => {
                        let mut out = tokio::io::stdout();
                        if out.write_all(buf.as_bytes()).await.is_err() {
                            return;
                        }
                        let _ = out.flush().await;
                    }
                    Err(_) => return,
                }
            }
        });
    }

    *child.lock().await = Some(new_child);
    Ok(())
}
