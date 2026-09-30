use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::os::unix::fs::{DirBuilderExt, symlink};
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use glimpse_config::{AppletKind, DATA_DIR, is_desktop_id, placed_applets};
use glimpse_services::{
    DesktopCatalog, Edge, ExecEntry, FromApplet, MAX_LINE, Orientation, Outgoing, Placement, Tree,
    Zone,
};
use minijinja::{AutoEscape, Environment, UndefinedBehavior, context};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::signal::unix::{SignalKind, signal};

use crate::cli::{NewAppletArgs, RunAppletArgs};
use crate::render::Section;

fn prompt(
    flag: &str,
    label: &str,
    supplied: Option<String>,
    default: Option<&str>,
    yes: bool,
    valid: impl Fn(&str) -> Result<()>,
) -> Result<String> {
    let tty = io::stdin().is_terminal();
    let mut supplied = supplied;
    loop {
        let value = match supplied.take() {
            Some(value) => value,
            None if yes || !tty => default
                .map(str::to_owned)
                .with_context(|| format!("missing --{flag}"))?,
            None => {
                eprint!(
                    "{label}{}: ",
                    default.map_or(String::new(), |d| format!(" [{d}]"))
                );
                io::stderr().flush()?;
                let mut line = String::new();
                io::stdin().read_line(&mut line)?;
                let line = line.trim().to_owned();
                if line.is_empty() {
                    default
                        .map(str::to_owned)
                        .with_context(|| format!("missing --{flag}"))?
                } else {
                    line
                }
            }
        };
        match valid(&value) {
            Ok(()) => return Ok(value),
            Err(error) if tty && !yes => eprintln!("--{flag}: {error}"),
            Err(error) => return Err(error).with_context(|| format!("invalid --{flag}")),
        }
    }
}

fn words(id: &str) -> (String, String) {
    let last = id.rsplit('.').next().unwrap_or(id);
    let mut spaced = String::new();
    let mut kebab = String::new();
    for (index, ch) in last.chars().enumerate() {
        if index > 0 && ch.is_uppercase() {
            spaced.push(' ');
            kebab.push('-');
        }
        spaced.push(ch);
        kebab.extend(ch.to_lowercase());
    }
    (spaced, kebab)
}

pub fn applets_run(args: RunAppletArgs) -> Result<()> {
    Err(anyhow::Error::new(crate::errors::ExecFailed(
        std::process::Command::new(&args.exe)
            .args(&args.args)
            .exec(),
    )))
    .with_context(|| format!("cannot execute {}", args.exe.display()))
}

fn desktop_value(value: &str) -> Result<()> {
    ensure!(
        !value.chars().any(|ch| ch.is_control() || ch == '\\'),
        "must be one line without backslashes"
    );
    Ok(())
}

pub fn applets_new(args: NewAppletArgs) -> Result<()> {
    let installed = DesktopCatalog.installed();
    let template = env::var_os("GLIMPSE_TS_SDK_TEMPLATE_DIR")
        .map_or_else(|| Path::new(DATA_DIR).join("sdk/template"), PathBuf::from);
    applets_new_with_catalog(args, &template, |id| {
        installed.iter().any(|(name, _)| name == id)
    })
}

pub fn applets_new_with_catalog(
    args: NewAppletArgs,
    template: &Path,
    installed: impl Fn(&str) -> bool,
) -> Result<()> {
    let id = prompt("id", "Applet id", args.id, None, args.yes, |id| {
        ensure!(is_desktop_id(id), "expected a desktop-file id");
        ensure!(!installed(id), "already installed");
        Ok(())
    })?;
    let (default_name, default_dir) = words(&id);
    let name = prompt(
        "name",
        "Name",
        args.name,
        Some(&default_name),
        args.yes,
        |v| {
            ensure!(!v.trim().is_empty(), "name is empty");
            desktop_value(v)
        },
    )?;
    let description = prompt(
        "description",
        "Description",
        args.description,
        Some(""),
        args.yes,
        desktop_value,
    )?;
    let icon = prompt(
        "icon",
        "Icon",
        args.icon,
        Some("application-x-addon-symbolic"),
        args.yes,
        |v| {
            ensure!(
                !v.is_empty()
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-')),
                "invalid icon name"
            );
            Ok(())
        },
    )?;
    let dir = prompt(
        "dir",
        "Directory",
        args.dir.map(|p| p.display().to_string()),
        Some(&default_dir),
        args.yes,
        |v| {
            ensure!(!v.is_empty(), "directory is empty");
            let path = Path::new(v);
            ensure!(
                !path.exists() || fs::read_dir(path)?.next().is_none(),
                "directory is not empty"
            );
            Ok(())
        },
    )?;
    let dir = PathBuf::from(dir);
    fs::create_dir_all(&dir)?;
    let dir = dir.canonicalize()?;
    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| AutoEscape::None);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_keep_trailing_newline(true);
    let context = context! { id, name, description, icon, dir };
    for item in fs::read_dir(template)
        .with_context(|| format!("applet template not installed at {}", template.display()))?
    {
        let path = item?.path();
        let file = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("template file name is not UTF-8")?;
        let file = env.render_str(file, &context)?;
        let rendered = env
            .render_str(&fs::read_to_string(&path)?, &context)
            .with_context(|| format!("cannot render {file}"))?;
        fs::write(dir.join(file), rendered)?;
    }
    let shown = env::current_dir()
        .ok()
        .and_then(|cwd| dir.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .filter(|relative| !relative.as_os_str().is_empty())
        .unwrap_or_else(|| dir.clone());
    println!("Created {id} in {}\n", shown.display());
    Section::new("next")
        .with(format!(
            "cd {}\nnpm install\nglimpsectl applets dev .",
            shown.display()
        ))
        .print()?;
    println!();
    config_section(&id, &dir, false)?.print()
}

fn config_section(id: &str, dir: &Path, placed: bool) -> Result<Section> {
    let path = serde_json::to_string(&dir.display().to_string())?;
    let section = Section::new("config.toml").with(format!(
        "[applets.\"{id}\"]\nsandbox = {{ read = [{path}], write = [{path}] }}"
    ));
    Ok(match placed {
        true => section,
        false => section.with_note(&format!("then add it to a panel zone: center = [\"{id}\"]")),
    })
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

async fn handshake(entry: &ExecEntry) -> Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let state = env::temp_dir().join(format!("glimpsectl-check-{}-{now}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&state)?;
    let result = handshake_in(entry, &state).await;
    let removed = fs::remove_dir_all(&state);
    result?;
    removed?;
    Ok(())
}

async fn handshake_in(entry: &ExecEntry, state: &Path) -> Result<()> {
    let (program, args) = entry.argv.split_first().context("empty Exec")?;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(entry.cwd.as_deref().unwrap_or(Path::new(".")))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdout = BufReader::new(child.stdout.take().context("no applet stdout")?);
    let mut stdin = child.stdin.take().context("no applet stdin")?;
    let result = async {
        let line = wire_line(&mut stdout, "Hello").await?;
        ensure!(
            matches!(
                serde_json::from_str::<FromApplet>(&line)?,
                FromApplet::Hello { v: 1 }
            ),
            "expected applet Hello v1"
        );
        let hello = Outgoing::Hello {
            v: 1,
            name: entry.name.clone(),
            state: state.display().to_string(),
            options: serde_json::Map::new(),
            placement: Placement {
                output: None,
                position: Edge::Top,
                orientation: Orientation::Horizontal,
                zone: Zone::Center,
                size: 32,
            },
        };
        stdin
            .write_all(serde_json::to_string(&hello)?.as_bytes())
            .await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        let line = wire_line(&mut stdout, "Commit").await?;
        let FromApplet::Commit { ops } = serde_json::from_str::<FromApplet>(&line)? else {
            bail!("expected applet Commit")
        };
        Tree::default()
            .apply(ops)
            .map_err(|error| anyhow::anyhow!("invalid applet tree: {error}"))?;
        Ok::<(), anyhow::Error>(())
    }
    .await;
    drop(stdin);
    let _ = child.kill().await;
    let _ = child.wait().await;
    if let Err(error) = result {
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr).await;
        }
        bail!("{error}\n{stderr}");
    }
    Ok(())
}

async fn wire_line(reader: &mut (impl AsyncBufRead + Unpin), phase: &str) -> Result<String> {
    let mut line = String::new();
    let size = tokio::time::timeout(
        Duration::from_secs(2),
        reader.take((MAX_LINE + 1) as u64).read_line(&mut line),
    )
    .await
    .with_context(|| format!("applet {phase} timed out"))??;
    ensure!(size > 0, "applet exited before {phase}");
    ensure!(
        size <= MAX_LINE && line.ends_with('\n'),
        "applet {phase} exceeds the wire line limit"
    );
    Ok(line)
}

pub async fn applets_dev(target: &str, config_path: Option<PathBuf>) -> Result<()> {
    if !Path::new(target).is_dir() {
        ensure!(
            is_desktop_id(target),
            "{target} is neither an applet directory nor a desktop-file id"
        );
        let entry = DesktopCatalog
            .installed()
            .into_iter()
            .find_map(|(id, entry)| (id == target).then_some(entry))
            .with_context(|| format!("{target} is not installed"))?
            .map_err(anyhow::Error::msg)?;
        handshake(&entry).await?;
        return super::applets_restart(target.to_owned()).await;
    }
    let dir = Path::new(target).canonicalize()?;
    let (id, entry) = source(&dir)?;
    handshake(&entry).await?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    let applications = applications_dir()?;
    fs::create_dir_all(&applications)?;
    let link = applications.join(format!("{id}.desktop"));
    if fs::read_link(&link).ok().as_deref() == Some(entry.path.as_path()) {
        fs::remove_file(&link)?;
    }
    ensure!(
        !link.exists() && !link.is_symlink(),
        "{} already exists",
        link.display()
    );
    symlink(&entry.path, &link)?;
    let placed = glimpse_config::load(config_path.as_deref())
        .ok()
        .is_some_and(|config| {
            placed_applets(&config).any(
                |(_, applet)| matches!(applet.kind, AppletKind::Exec(exec) if exec.applet == id),
            )
        });
    config_section(&id, &dir, placed)?.print()?;
    println!("\nLinked {}. Press Ctrl-C to unlink.", link.display());
    eprintln!("logs: journalctl --user -u glimpse-panel -f");
    tokio::select! {
        _ = interrupt.recv() => {},
        _ = terminate.recv() => {},
        _ = hangup.recv() => {},
    }
    if fs::read_link(&link).ok().as_deref() == Some(entry.path.as_path()) {
        fs::remove_file(link)?;
    }
    Ok(())
}

fn home() -> Result<PathBuf> {
    let home = env::var_os("HOME").context("HOME is unset")?;
    let home = PathBuf::from(home);
    ensure!(home.is_absolute(), "HOME must be absolute");
    Ok(home)
}

fn applications_dir() -> Result<PathBuf> {
    let path = match env::var_os("XDG_DATA_HOME") {
        Some(path) => PathBuf::from(path),
        None => home()?.join(".local/share"),
    };
    ensure!(path.is_absolute(), "XDG_DATA_HOME must be absolute");
    Ok(path.join("applications"))
}
