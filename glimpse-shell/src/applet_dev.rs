//! `glimpse-shell applets dev` — dev-mode supervisor ported from
//! glimpse-applets-bin. Registers a transient `<id>.dev.toml`, then (for
//! exec applets) builds + watches + respawns the program and proxies stdio
//! between the shell and the child, replaying the cached `init` line after
//! every rebuild. On build failure / unexpected child exit it answers the
//! shell protocol itself with a ⚠ error applet so the breakage is visible
//! in the panel instead of only the terminal.
//!
//! Registration never modifies the project's applet.toml:
//!   - command applets: symlink applet.toml -> <id>.dev.toml
//!   - exec applets:    write a copy with only `exec.command`/`work_dir`
//!                      patched to invoke this supervisor
//! Both live in the `.dev` namespace and are removed on exit.

use anyhow::{Context, Result, bail};
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Language {
    Rust,
    Python,
    Typescript,
    Go,
}

impl Language {
    fn manifest(self) -> &'static str {
        match self {
            Self::Rust => "Cargo.toml",
            Self::Python => "pyproject.toml",
            Self::Typescript => "package.json",
            Self::Go => "go.mod",
        }
    }
    fn entrypoint(self) -> &'static str {
        match self {
            Self::Rust => "src/main.rs",
            Self::Python => "main.py",
            Self::Typescript => "src/main.ts",
            Self::Go => "main.go",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Typescript => "typescript",
            Self::Go => "go",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "rust" => Some(Self::Rust),
            "python" => Some(Self::Python),
            "typescript" => Some(Self::Typescript),
            "go" => Some(Self::Go),
            _ => None,
        }
    }
    fn detect(dir: &Path) -> Result<Self> {
        let all = [Self::Rust, Self::Python, Self::Typescript, Self::Go];
        let by_manifest: Vec<Self> = all
            .iter()
            .copied()
            .filter(|l| dir.join(l.manifest()).is_file())
            .collect();
        match by_manifest.len() {
            1 => return Ok(by_manifest[0]),
            n if n > 1 => bail!(
                "multiple language manifests in {}: {}. pass --lang to choose.",
                dir.display(),
                by_manifest
                    .iter()
                    .map(|l| l.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            _ => {}
        }
        let by_entry: Vec<Self> = all
            .iter()
            .copied()
            .filter(|l| dir.join(l.entrypoint()).is_file())
            .collect();
        match by_entry.len() {
            1 => Ok(by_entry[0]),
            0 => bail!(
                "no language manifest in {} (expected Cargo.toml, pyproject.toml, package.json, or go.mod). pass --lang.",
                dir.display()
            ),
            _ => bail!(
                "multiple entry points in {}: {}. pass --lang to choose.",
                dir.display(),
                by_entry
                    .iter()
                    .map(|l| l.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

fn log(msg: impl AsRef<str>) {
    // stdout is the applet protocol stream the shell parses — talk on stderr.
    eprintln!("[glimpse-shell applets dev] {}", msg.as_ref());
}

fn applets_dir() -> PathBuf {
    glimpse_core::AppletDirectoryScanner::from_process().user_dir
}

pub fn print_help() {
    println!("glimpse-shell-applets-dev");
    println!("Run an applet in dev mode: live rebuild + reload, errors shown in the panel");
    println!();
    println!("USAGE:");
    println!("    glimpse-shell applets dev [OPTIONS] [PATH]");
    println!();
    println!("ARGS:");
    println!("    [PATH]   Project directory (default: .)");
    println!();
    println!("OPTIONS:");
    println!("    --lang <rust|python|typescript|go>   Override language detection");
    println!("    --debounce-ms <N>                    File-change debounce (default: 300)");
    println!("    -h, --help                           Print help");
}

pub async fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }

    let mut path: Option<PathBuf> = None;
    let mut lang_override: Option<Language> = None;
    let mut debounce_ms: u64 = 300;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--lang" => {
                let v = it.next().context("--lang requires a value")?;
                lang_override =
                    Some(Language::parse(v).with_context(|| {
                        format!("--lang must be rust|python|typescript|go, got {v:?}")
                    })?);
            }
            "--debounce-ms" => {
                debounce_ms = it
                    .next()
                    .context("--debounce-ms requires a value")?
                    .parse()
                    .context("--debounce-ms must be a non-negative integer")?;
            }
            other if other.starts_with('-') => bail!("unknown option: {other}"),
            positional => {
                if path.is_some() {
                    bail!("unexpected extra argument: {positional}");
                }
                path = Some(PathBuf::from(positional));
            }
        }
    }

    let path = path
        .unwrap_or_else(|| PathBuf::from("."))
        .canonicalize()
        .context("resolve project path")?;

    let (applet_id, applet_type) = read_applet_meta(&path)
        .context("read applet.toml (run from / point at an applet project)")?;

    // Only the standalone invocation (interactive stdin) owns the dev-config
    // file lifecycle. When the shell spawns us via the registered exec command
    // our stdin is a pipe — that instance must not register or remove it.
    let standalone = std::io::stdin().is_terminal();

    let _guard = if standalone {
        match register_dev_config(&applet_id, &applet_type, &path) {
            Ok(p) => {
                check_dev_panel_config(&applet_id);
                Some(DevConfigGuard(p))
            }
            Err(e) => {
                log(format!("warning: could not register dev config: {e:#}"));
                None
            }
        }
    } else {
        None
    };

    if applet_type == AppletType::Command {
        // Nothing to build/run — command applets are pure config. Keep the
        // process alive so the registration persists for the session.
        log(format!(
            "command applet '{applet_id}' registered for dev; edit applet.toml and it reloads. Ctrl-C to stop."
        ));
        wait_for_shutdown().await;
        return Ok(());
    }

    let lang = match lang_override {
        Some(l) => l,
        None => Language::detect(&path)?,
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

    supervise(plan, debounce_ms, !standalone).await
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppletType {
    Exec,
    Command,
}

fn read_applet_meta(dir: &Path) -> Result<(String, AppletType)> {
    let content = std::fs::read_to_string(dir.join("applet.toml")).context("read applet.toml")?;
    let value: toml::Value = toml::from_str(&content).context("parse applet.toml")?;
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .context("applet.toml has no non-empty `id`")?;
    let ty = match value.get("type").and_then(|v| v.as_str()) {
        Some("command") => AppletType::Command,
        Some("exec") => AppletType::Exec,
        Some(other) => bail!("applet.toml has unknown type {other:?} (expected exec or command)"),
        None => bail!("applet.toml has no `type` (expected type = \"exec\" or \"command\")"),
    };
    Ok((id, ty))
}

fn register_dev_config(id: &str, ty: &AppletType, project_dir: &Path) -> Result<PathBuf> {
    let dir = applets_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create applets dir {}", dir.display()))?;
    let dest = dir.join(format!("{id}.dev.toml"));
    let supervisor = std::env::current_exe().context("resolve current executable")?;
    materialize_dev_config(&dest, ty, project_dir, &supervisor)?;
    log(format!("registered dev config {}", dest.display()));
    Ok(dest)
}

/// Write `<dest>` for a dev session. command → symlink the project's
/// applet.toml (edits reflect live, original untouched). exec → a copy of
/// applet.toml with ONLY `exec.command`/`exec.work_dir` patched to invoke
/// `supervisor`; every other key/table is preserved and the original file is
/// never modified.
fn materialize_dev_config(
    dest: &Path,
    ty: &AppletType,
    project_dir: &Path,
    supervisor: &Path,
) -> Result<()> {
    if dest.exists() || dest.is_symlink() {
        std::fs::remove_file(dest).with_context(|| format!("remove stale {}", dest.display()))?;
    }
    match ty {
        AppletType::Command => {
            let src = project_dir.join("applet.toml");
            std::os::unix::fs::symlink(&src, dest)
                .with_context(|| format!("symlink {} -> {}", dest.display(), src.display()))?;
        }
        AppletType::Exec => {
            let original = std::fs::read_to_string(project_dir.join("applet.toml"))
                .context("read applet.toml")?;
            let mut value: toml::Value =
                toml::from_str(&original).context("parse applet.toml")?;
            let table = value
                .as_table_mut()
                .context("applet.toml root is not a table")?;
            let exec = table
                .entry("exec".to_string())
                .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
                .as_table_mut()
                .context("applet.toml [exec] is not a table")?;
            exec.insert(
                "command".into(),
                toml::Value::Array(vec![
                    toml::Value::String(supervisor.display().to_string()),
                    toml::Value::String("applets".into()),
                    toml::Value::String("dev".into()),
                    toml::Value::String(project_dir.display().to_string()),
                ]),
            );
            exec.insert(
                "work_dir".into(),
                toml::Value::String(project_dir.display().to_string()),
            );
            let body = toml::to_string(&value).context("serialize dev config")?;
            std::fs::write(
                dest,
                format!("# Generated by `glimpse-shell applets dev` — do not edit\n{body}"),
            )
            .with_context(|| format!("write {}", dest.display()))?;
        }
    }
    Ok(())
}

struct DevConfigGuard(PathBuf);

impl Drop for DevConfigGuard {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.0) {
            Ok(()) => eprintln!(
                "[glimpse-shell applets dev] removed dev config {}",
                self.0.display()
            ),
            Err(e) => eprintln!(
                "[glimpse-shell applets dev] warning: could not remove {}: {e}",
                self.0.display()
            ),
        }
    }
}

fn check_dev_panel_config(applet_id: &str) {
    let cfg = applets_dir()
        .parent()
        .map(|p| p.join("config.toml"))
        .unwrap_or_default();
    let Ok(content) = std::fs::read_to_string(&cfg) else {
        return;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&content) else {
        return;
    };
    let has_dev = value
        .get("panels")
        .and_then(|v| v.as_array())
        .map(|panels| {
            panels.iter().any(|panel| {
                ["left", "center", "right"].iter().any(|s| {
                    panel
                        .get(s)
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().any(|i| i.as_str() == Some("__dev__")))
                        .unwrap_or(false)
                })
            })
        })
        .unwrap_or(false);
    if !has_dev {
        log(format!(
            "hint: add \"__dev__\" to a panel's left/center/right in {} to see {applet_id} in the bar",
            cfg.display()
        ));
    }
}

struct Plan {
    binary: String,
    args: Vec<String>,
    workdir: PathBuf,
    build: Option<(String, Vec<String>)>,
    watch_dirs: Vec<PathBuf>,
}

fn build_plan(lang: Language, path: &Path) -> Plan {
    match lang {
        Language::Rust => Plan {
            binary: "cargo".into(),
            args: vec!["run".into(), "--quiet".into()],
            workdir: path.to_path_buf(),
            build: Some(("cargo".into(), vec!["build".into(), "--quiet".into()])),
            watch_dirs: vec![path.join("src"), path.join("Cargo.toml")],
        },
        Language::Python => Plan {
            binary: "uv".into(),
            args: vec!["run".into(), "main.py".into()],
            workdir: path.to_path_buf(),
            build: None,
            watch_dirs: vec![path.join("main.py")],
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

async fn supervise(plan: Plan, debounce_ms: u64, surface_errors: bool) -> Result<()> {
    let init_line: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    // Captured failure text while no healthy child is running.
    let failure: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let (rebuild_tx, mut rebuild_rx) = mpsc::channel::<()>(8);
    let _watcher = start_watcher(&plan.watch_dirs, debounce_ms, rebuild_tx.clone())?;

    if let Err(e) = rebuild_and_respawn(&plan, &child, &init_line, &failure, surface_errors).await {
        log(format!("initial build failed: {e:#}"));
    }

    // stdin proxy: shell -> (child | error responder). Caches `init`.
    let mut stdin_task = {
        let child = child.clone();
        let init_line = init_line.clone();
        let failure = failure.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(tokio::io::stdin());
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf).await {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(e) => {
                        log(format!("stdin read error: {e}"));
                        break;
                    }
                }
                if buf.starts_with("init ") {
                    *init_line.lock().await = Some(buf.clone());
                }
                let has_child = child.lock().await.is_some();
                if has_child {
                    forward_to_child(&child, buf.as_bytes()).await;
                } else if surface_errors {
                    // No healthy child: answer the protocol ourselves.
                    if let Some(msg) = failure.lock().await.clone() {
                        respond_with_error(&buf, &msg).await;
                    }
                }
            }
            log("shell stdin closed; exiting");
        })
    };

    let (term_tx, mut term_rx) = mpsc::channel::<()>(1);
    #[cfg(unix)]
    if let Ok(mut sig) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        tokio::spawn(async move {
            if sig.recv().await.is_some() {
                let _ = term_tx.send(()).await;
            }
        });
    }

    loop {
        tokio::select! {
            biased;
            _ = &mut stdin_task => break,
            _ = tokio::signal::ctrl_c() => break,
            _ = term_rx.recv() => break,
            Some(()) = rebuild_rx.recv() => {
                while rebuild_rx.try_recv().is_ok() {}
                if let Err(e) =
                    rebuild_and_respawn(&plan, &child, &init_line, &failure, surface_errors).await
                {
                    log(format!("rebuild failed: {e:#}"));
                }
            }
        }
    }

    if let Some(mut c) = child.lock().await.take() {
        let _ = c.start_kill();
        let _ = c.wait().await;
    }
    Ok(())
}

async fn wait_for_shutdown() {
    let (term_tx, mut term_rx) = mpsc::channel::<()>(1);
    #[cfg(unix)]
    if let Ok(mut sig) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        tokio::spawn(async move {
            if sig.recv().await.is_some() {
                let _ = term_tx.send(()).await;
            }
        });
    }
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term_rx.recv() => {}
    }
}

/// Emit a ⚠ error applet on stdout in response to the shell's protocol.
/// `status` is pushed proactively; the popover tree is sent when the shell
/// reports the popover opened.
async fn respond_with_error(incoming: &str, message: &str) {
    let mut out = tokio::io::stdout();
    let short = message.lines().next_back().unwrap_or("build failed");
    let status = serde_json::json!({
        "items": [{
            "id": "glimpse-dev-error",
            "icon": { "name": "dialog-error-symbolic" },
            "label": "dev: failed",
            "tooltip": short,
        }]
    });
    let _ = out
        .write_all(format!("status {status}\n").as_bytes())
        .await;

    // The host sends `event {"id":"popover","type":"open",...}` when opened.
    if incoming.starts_with("event ")
        && incoming.contains("\"id\":\"popover\"")
        && incoming.contains("\"type\":\"open\"")
    {
        let popover = serde_json::json!({
            "root": {
                "type": "section",
                "data": {
                    "title": "Applet build failed",
                    "subtitle": "glimpse-shell applets dev",
                    "children": [{
                        "type": "label",
                        "data": { "text": message, "wrap": true, "selectable": true }
                    }]
                }
            }
        });
        let _ = out
            .write_all(format!("popover {popover}\n").as_bytes())
            .await;
    }
    let _ = out.flush().await;
}

async fn forward_to_child(child: &Arc<Mutex<Option<Child>>>, bytes: &[u8]) {
    let mut guard = child.lock().await;
    if let Some(c) = guard.as_mut() {
        if let Some(stdin) = c.stdin.as_mut() {
            if let Err(e) = stdin.write_all(bytes).await {
                log(format!("forward to child failed: {e}"));
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
                    if matches!(
                        e.kind,
                        notify::EventKind::Access(_)
                            | notify::EventKind::Modify(notify::event::ModifyKind::Metadata(_))
                            | notify::EventKind::Other
                    ) {
                        return false;
                    }
                    !e.paths.iter().any(|p| {
                        let s = p.to_string_lossy();
                        s.contains("/target/")
                            || s.contains("/dist/")
                            || s.contains("/node_modules/")
                            || s.contains("/.git/")
                            || s.contains("__pycache__")
                            || s.contains("/.venv/")
                            || s.ends_with(".dev-build")
                            || s.ends_with(".lock")
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
    child: &Arc<Mutex<Option<Child>>>,
    init_line: &Arc<Mutex<Option<String>>>,
    failure: &Arc<Mutex<Option<String>>>,
    surface_errors: bool,
) -> Result<()> {
    if let Some((cmd, args)) = &plan.build {
        log(format!("building: {cmd} {}", args.join(" ")));
        let output = TokioCommand::new(cmd)
            .args(args)
            .current_dir(&plan.workdir)
            .output()
            .await
            .with_context(|| format!("invoke {cmd}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let detail = if stderr.trim().is_empty() {
                format!("build exited with {}", output.status)
            } else {
                stderr
            };
            // Surface the failure in the panel (kill the stale child first so
            // the error responder takes over).
            if let Some(mut c) = child.lock().await.take() {
                let _ = c.start_kill();
                let _ = c.wait().await;
            }
            if surface_errors {
                *failure.lock().await = Some(detail.clone());
                respond_with_error("", &detail).await;
            }
            bail!("build failed");
        }
    }

    if let Some(mut c) = child.lock().await.take() {
        let _ = c.start_kill();
        let _ = c.wait().await;
    }

    log(format!("starting: {} {}", plan.binary, plan.args.join(" ")));
    let mut new_child = TokioCommand::new(&plan.binary)
        .args(&plan.args)
        .current_dir(&plan.workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("spawn {}", plan.binary))?;

    if let Some(init) = init_line.lock().await.clone() {
        if let Some(stdin) = new_child.stdin.as_mut() {
            stdin
                .write_all(init.as_bytes())
                .await
                .context("replay init to child")?;
        }
    }

    if let Some(stdout) = new_child.stdout.take() {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf).await {
                    Ok(0) => return,
                    Ok(_) => {
                        let mut o = tokio::io::stdout();
                        if o.write_all(buf.as_bytes()).await.is_err() {
                            return;
                        }
                        let _ = o.flush().await;
                    }
                    Err(_) => return,
                }
            }
        });
    }

    // Healthy child: clear any prior failure state.
    *failure.lock().await = None;
    *child.lock().await = Some(new_child);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp(name: &str) -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("glimpse-dev-test-{name}-{n}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn language_detect_and_parse() {
        assert_eq!(Language::parse("go").map(|l| l.name()), Some("go"));
        assert!(Language::parse("cobol").is_none());
        let d = tmp("detect");
        std::fs::write(d.join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(Language::detect(&d).unwrap().name(), "rust");
        std::fs::write(d.join("go.mod"), "module x").unwrap();
        assert!(Language::detect(&d).is_err()); // ambiguous
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn exec_dev_config_is_patched_copy_preserving_keys() {
        let proj = tmp("exec-proj");
        std::fs::write(
            proj.join("applet.toml"),
            "id = \"demo\"\ntype = \"exec\"\n\n[exec]\ncommand = [\"/usr/bin/demo\"]\nenv = { FOO = \"bar\" }\n\n[settings]\nrefresh_ms = 1234\n",
        )
        .unwrap();
        let original = std::fs::read_to_string(proj.join("applet.toml")).unwrap();
        let dest_dir = tmp("exec-dest");
        let dest = dest_dir.join("demo.dev.toml");
        let supervisor = PathBuf::from("/opt/glimpse-shell");

        materialize_dev_config(&dest, &AppletType::Exec, &proj, &supervisor).unwrap();

        let written = std::fs::read_to_string(&dest).unwrap();
        let v: toml::Value = toml::from_str(&written).unwrap();
        let cmd = v["exec"]["command"].as_array().unwrap();
        assert_eq!(cmd[0].as_str(), Some("/opt/glimpse-shell"));
        assert_eq!(cmd[1].as_str(), Some("applets"));
        assert_eq!(cmd[2].as_str(), Some("dev"));
        assert_eq!(cmd[3].as_str(), Some(proj.to_str().unwrap()));
        assert_eq!(
            v["exec"]["work_dir"].as_str(),
            Some(proj.to_str().unwrap())
        );
        // Unrelated keys preserved.
        assert_eq!(v["settings"]["refresh_ms"].as_integer(), Some(1234));
        assert_eq!(v["exec"]["env"]["FOO"].as_str(), Some("bar"));
        assert_eq!(v["id"].as_str(), Some("demo"));
        // Original never modified, dest is a real file not a symlink.
        assert_eq!(
            std::fs::read_to_string(proj.join("applet.toml")).unwrap(),
            original
        );
        assert!(!std::fs::symlink_metadata(&dest).unwrap().is_symlink());
        std::fs::remove_dir_all(&proj).ok();
        std::fs::remove_dir_all(&dest_dir).ok();
    }

    #[test]
    fn command_dev_config_is_symlink_to_original() {
        let proj = tmp("cmd-proj");
        let src = proj.join("applet.toml");
        std::fs::write(&src, "id = \"c\"\ntype = \"command\"\n").unwrap();
        let dest_dir = tmp("cmd-dest");
        let dest = dest_dir.join("c.dev.toml");

        materialize_dev_config(&dest, &AppletType::Command, &proj, &PathBuf::from("/x"))
            .unwrap();

        assert!(std::fs::symlink_metadata(&dest).unwrap().is_symlink());
        assert_eq!(std::fs::read_link(&dest).unwrap(), src);
        // Re-materializing replaces a stale entry cleanly.
        materialize_dev_config(&dest, &AppletType::Command, &proj, &PathBuf::from("/x"))
            .unwrap();
        assert_eq!(std::fs::read_link(&dest).unwrap(), src);
        std::fs::remove_dir_all(&proj).ok();
        std::fs::remove_dir_all(&dest_dir).ok();
    }
}
