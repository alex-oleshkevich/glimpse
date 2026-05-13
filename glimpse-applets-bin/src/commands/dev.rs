//! Dev-mode supervisor: builds + watches + spawns the applet binary, and
//! proxies stdio between Glimpse and the applet. On source change, we
//! rebuild and respawn the applet; the cached `init` line is replayed to
//! the new child so it starts the same way Glimpse would have.

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use notify::RecursiveMode;
use notify_debouncer_full::new_debouncer;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::{Mutex, mpsc};

use crate::project::Language;

#[derive(ClapArgs)]
pub struct Args {
    /// Project directory (containing Cargo.toml / pyproject.toml /
    /// package.json / go.mod). Defaults to the current directory.
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
    let stdin_task = {
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

    // Supervisor loop: handle file change events.
    loop {
        tokio::select! {
            biased;
            _ = &mut Box::pin(stdin_task_done(&stdin_task)) => break,
            Some(()) = rebuild_rx.recv() => {
                // Drain any further bursts.
                while rebuild_rx.try_recv().is_ok() {}
                if let Err(e) = rebuild_and_respawn(&plan, child.clone(), init_line.clone()).await {
                    log(format!("rebuild failed: {e:#}"));
                }
            }
        }
    }
    Ok(())
}

async fn stdin_task_done(handle: &tokio::task::JoinHandle<()>) {
    // poll without panicking: if it's finished, return; otherwise wait
    // for it to finish.
    if handle.is_finished() {
        return;
    }
    std::future::pending::<()>().await;
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
) -> Result<notify_debouncer_full::Debouncer<notify::RecommendedWatcher, notify_debouncer_full::RecommendedCache>>
{
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
    let mut new_child = cmd.spawn().with_context(|| format!("spawn {}", plan.binary))?;

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

