use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use glimpse_config::{AppletKind, DATA_DIR, is_desktop_id, placed_applets};
use glimpse_services::{
    DesktopCatalog, Edge, ExecEntry, FromApplet, MAX_LINE, Orientation, Outgoing, Placement, Tree,
    Zone,
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::cli::NewAppletArgs;

const MAIN: &str = include_str!("../../../../sdk/applet/template/main.tsx");
const INDICATOR: &str = include_str!("../../../../sdk/applet/template/main.indicator.tsx");
const DENO: &str = include_str!("../../../../sdk/applet/template/deno.json");
const DESKTOP: &str = include_str!("../../../../sdk/applet/template/applet.desktop");
const README: &str = include_str!("../../../../sdk/applet/template/README.md");
const GITIGNORE: &str = include_str!("../../../../sdk/applet/template/gitignore");

struct Answers {
    id: String,
    name: String,
    description: String,
    icon: String,
    popover: bool,
    allow_net: String,
    dir: PathBuf,
}

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
        let checked = if value.contains("{{") {
            Err(anyhow::anyhow!("template delimiters are not allowed"))
        } else {
            valid(&value)
        };
        match checked {
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

fn desktop_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn exec_quote(value: &str) -> String {
    let value = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
        .replace('%', "%%");
    format!("\"{value}\"")
}

fn launcher() -> String {
    if let Ok(path) = env::current_exe() {
        let sibling = path.with_file_name("glimpse-applet");
        if sibling.is_file() {
            return sibling.display().to_string();
        }
    }
    "glimpse-applet".to_owned()
}

fn launcher_exec() -> String {
    let launcher = launcher();
    if launcher.contains([' ', '"', '\\', '%']) {
        exec_quote(&launcher)
    } else {
        launcher
    }
}

fn sdk_path() -> PathBuf {
    let installed = Path::new(DATA_DIR).join("sdk/applet/mod.ts");
    if installed.exists() {
        return installed;
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/applet/mod.ts")
}

fn render(template: &str, answers: &Answers) -> String {
    let dir = answers.dir.display().to_string();
    let exec = format!(
        "{} --watch{} {}",
        launcher_exec(),
        if answers.allow_net.is_empty() {
            String::new()
        } else {
            format!(" --allow-net={}", answers.allow_net)
        },
        exec_quote(&answers.dir.join("main.tsx").display().to_string()),
    );
    let sdk = format!("file://{}", sdk_path().display());
    let js = template == MAIN || template == INDICATOR;
    let values = [
        ("{{id}}", answers.id.clone()),
        (
            "{{name}}",
            if js {
                serde_json::to_string(&answers.name).unwrap_or_default()
            } else {
                desktop_escape(&answers.name)
            },
        ),
        ("{{description}}", desktop_escape(&answers.description)),
        (
            "{{icon}}",
            if js {
                serde_json::to_string(&answers.icon).unwrap_or_default()
            } else {
                answers.icon.clone()
            },
        ),
        ("{{exec}}", exec),
        ("{{dir}}", dir),
        ("{{sdk}}", serde_json::to_string(&sdk).unwrap_or_default()),
    ];
    values
        .into_iter()
        .fold(template.to_owned(), |text, (key, value)| {
            text.replace(key, &value)
        })
}

pub fn applets_new(args: NewAppletArgs) -> Result<()> {
    let id = prompt("id", "Applet id", args.id, None, args.yes, |id| {
        ensure!(is_desktop_id(id), "expected a desktop-file id");
        ensure!(
            !DesktopCatalog
                .installed()
                .iter()
                .any(|(installed, _)| installed == id),
            "already installed"
        );
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
            Ok(())
        },
    )?;
    let description = prompt(
        "description",
        "Description",
        args.description,
        Some(""),
        args.yes,
        |v| {
            ensure!(!v.contains(['\n', '\r']), "must be one line");
            Ok(())
        },
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
    let popover = if args.no_popover {
        false
    } else if args.popover || args.yes || !io::stdin().is_terminal() {
        true
    } else {
        prompt(
            "popover",
            "Popover? (yes/no)",
            None,
            Some("yes"),
            false,
            |v| {
                ensure!(matches!(v, "yes" | "no"), "answer yes or no");
                Ok(())
            },
        )? == "yes"
    };
    let allow_net = prompt(
        "allow-net",
        "Network hosts",
        args.allow_net,
        Some(""),
        args.yes,
        |v| {
            ensure!(
                v.is_empty()
                    || v.split(',').all(|host| !host.is_empty()
                        && host.bytes().all(|b| b.is_ascii_alphanumeric()
                            || matches!(b, b'.' | b'-' | b':' | b'[' | b']'))),
                "invalid comma-separated hosts"
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
    let answers = Answers {
        id,
        name,
        description,
        icon,
        popover,
        allow_net,
        dir: dir.clone(),
    };
    let files = [
        ("main.tsx", if answers.popover { MAIN } else { INDICATOR }),
        ("deno.json", DENO),
        ("README.md", README),
        (".gitignore", GITIGNORE),
    ];
    for (name, template) in files {
        let rendered = render(template, &answers);
        ensure!(
            !rendered.contains("{{"),
            "unresolved template key in {name}"
        );
        fs::write(dir.join(name), rendered)?;
    }
    let rendered = render(DESKTOP, &answers);
    ensure!(!rendered.contains("{{"), "unresolved desktop template key");
    fs::write(dir.join(format!("{}.desktop", answers.id)), rendered)?;
    println!(
        "Created {}. Run `glimpsectl applets dev {}` and add `center = [\"{}\"]` to your panel config.",
        answers.id,
        dir.display(),
        answers.id
    );
    Ok(())
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

async fn deno_check(dir: &Path) -> Result<()> {
    let deno = glimpse_utils::deno().context("Deno not found; set GLIMPSE_DENO or install deno")?;
    let output = Command::new(deno)
        .arg("check")
        .arg("main.tsx")
        .current_dir(dir)
        .output()
        .await?;
    ensure!(
        output.status.success(),
        "deno check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

async fn handshake(entry: &ExecEntry) -> Result<()> {
    let (program, args) = entry.argv.split_first().context("empty Exec")?;
    let mut child = Command::new(program)
        .args(args.iter().filter(|arg| arg.as_str() != "--watch"))
        .current_dir(
            entry
                .cwd
                .as_deref()
                .unwrap_or_else(|| entry.path.parent().unwrap_or(Path::new("."))),
        )
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

pub async fn applets_check(dir: &Path) -> Result<()> {
    let (_, entry) = source(dir)?;
    deno_check(dir).await?;
    handshake(&entry).await?;
    println!("Applet check passed");
    Ok(())
}

pub async fn applets_dev(dir: &Path, config_path: Option<PathBuf>) -> Result<()> {
    let dir = dir.canonicalize()?;
    let (id, entry) = source(&dir)?;
    handshake(&entry).await?;
    static STOP: AtomicBool = AtomicBool::new(false);
    extern "C" fn stop(_: libc::c_int) {
        STOP.store(true, Ordering::SeqCst);
    }
    STOP.store(false, Ordering::SeqCst);
    ensure!(
        unsafe { libc::signal(libc::SIGINT, stop as *const () as libc::sighandler_t) }
            != libc::SIG_ERR,
        "failed to register SIGINT handler"
    );
    ensure!(
        unsafe { libc::signal(libc::SIGTERM, stop as *const () as libc::sighandler_t) }
            != libc::SIG_ERR,
        "failed to register SIGTERM handler"
    );
    let data_home = match env::var_os("XDG_DATA_HOME") {
        Some(path) => PathBuf::from(path),
        None => home()?.join(".local/share"),
    };
    ensure!(data_home.is_absolute(), "XDG_DATA_HOME must be absolute");
    let applications = data_home.join("applications");
    fs::create_dir_all(&applications)?;
    let link = applications.join(format!("{id}.desktop"));
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
    if !placed {
        println!("Add `center = [\"{id}\"]` to a panel applet zone.");
    }
    println!("Linked {}. Press Ctrl-C to unlink.", link.display());
    while !STOP.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(100)).await;
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

async fn bundle(dir: &Path, prefix: &Path, out: &Path) -> Result<()> {
    ensure!(prefix.is_absolute(), "--prefix must be absolute");
    let out = if out.is_absolute() {
        out.to_owned()
    } else {
        env::current_dir()?.join(out)
    };
    let (id, entry) = source(dir)?;
    deno_check(dir).await?;
    handshake(&entry).await?;
    let target = out.join("share").join(&id).join("applet.js");
    fs::create_dir_all(target.parent().context("bundle path has no parent")?)?;
    let deno = glimpse_utils::deno().context("Deno not found")?;
    let output = Command::new(deno)
        .arg("bundle")
        .arg("--minify")
        .arg("main.tsx")
        .arg("-o")
        .arg(&target)
        .current_dir(dir)
        .output()
        .await?;
    ensure!(
        output.status.success(),
        "deno bundle failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut argv = entry.argv.clone();
    argv.retain(|arg| arg != "--watch");
    if let Some(last) = argv.last_mut() {
        *last = prefix
            .join("share")
            .join(&id)
            .join("applet.js")
            .display()
            .to_string();
    }
    argv[0] = "glimpse-applet".to_owned();
    let command = argv
        .iter()
        .map(|arg| {
            if arg.contains([' ', '"', '\\', '%']) {
                exec_quote(arg)
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let source = fs::read_to_string(&entry.path)?;
    let desktop = source
        .lines()
        .filter(|line| !line.starts_with("Exec=") && !line.starts_with("TryExec="))
        .chain(["TryExec=glimpse-applet"])
        .collect::<Vec<_>>()
        .join("\n")
        + &format!("\nExec={command}\n");
    let destination = out.join("share/applications").join(format!("{id}.desktop"));
    fs::create_dir_all(destination.parent().context("desktop path has no parent")?)?;
    fs::write(destination, desktop)?;
    let icon = dir.join("icon.svg");
    if icon.is_file() {
        let destination = out
            .join("share/icons/hicolor/scalable/apps")
            .join(format!("{id}.svg"));
        fs::create_dir_all(destination.parent().context("icon path has no parent")?)?;
        fs::copy(icon, destination)?;
    }
    println!("Bundled {id} under {}", out.display());
    Ok(())
}

pub async fn applets_bundle(dir: &Path, prefix: &Path, out: &Path) -> Result<()> {
    bundle(dir, prefix, out).await
}

pub async fn applets_install(dir: &Path) -> Result<()> {
    let prefix = home()?.join(".local");
    bundle(dir, &prefix, &prefix).await
}

pub fn applets_uninstall(id: &str) -> Result<()> {
    ensure!(is_desktop_id(id), "invalid desktop-file id: {id}");
    let prefix = home()?.join(".local");
    let desktop = prefix
        .join("share/applications")
        .join(format!("{id}.desktop"));
    ensure!(
        desktop.is_file(),
        "user-installed desktop entry not found: {}",
        desktop.display()
    );
    let real_prefix = prefix.canonicalize()?;
    let real = desktop.canonicalize()?;
    ensure!(
        real.starts_with(&real_prefix),
        "refusing to remove an entry outside {}",
        prefix.display()
    );
    for path in [
        prefix.join("share").join(id).join("applet.js"),
        desktop,
        prefix
            .join("share/icons/hicolor/scalable/apps")
            .join(format!("{id}.svg")),
    ] {
        if path.exists() {
            ensure!(
                path.canonicalize()?.starts_with(&real_prefix),
                "refusing to remove {} outside {}",
                path.display(),
                prefix.display()
            );
            fs::remove_file(path)?;
        }
    }
    let _ = fs::remove_dir(prefix.join("share").join(id));
    println!("Uninstalled {id}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_renders_a_valid_scaffold_without_placeholders() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("hello");
        applets_new(NewAppletArgs {
            id: Some("me.example.Hello".to_owned()),
            name: Some("Hello Applet".to_owned()),
            description: Some("A test applet".to_owned()),
            icon: None,
            popover: false,
            no_popover: false,
            allow_net: Some("api.example".to_owned()),
            dir: Some(dir.clone()),
            yes: true,
        })
        .unwrap();
        for name in [
            "main.tsx",
            "deno.json",
            "me.example.Hello.desktop",
            "README.md",
            ".gitignore",
        ] {
            assert!(!fs::read_to_string(dir.join(name)).unwrap().contains("{{"));
        }
        let desktop = fs::read_to_string(dir.join("me.example.Hello.desktop")).unwrap();
        assert!(desktop.contains("--watch --allow-net=api.example"));
        assert!(desktop.contains("Implements=me.aresa.Glimpse.Applet1\n"));
        assert!(
            fs::read_to_string(dir.join("main.tsx"))
                .unwrap()
                .contains("<Popover>")
        );
    }
}
