//! `glimpse-shell applets {link,unlink,doctor}` — ported from glimpse-applets-bin.
//!
//! Improvements over the original:
//!  - `link` validates the applet.toml (id + type + matching section) before
//!    symlinking, so a broken applet fails loudly instead of being silently
//!    dropped by discovery; reports the resolved kind; `--force` replaces a
//!    conflicting regular file.
//!  - `unlink` accepts a bare `<id>` (not just a project path), so an applet
//!    can be unlinked after its project directory is gone.
//!  - `doctor` checks the on-disk applet-templates dir (new requirement for
//!    `applets new`) and the applets dir, drops the stale watchexec check
//!    (the ported `dev` uses notify, no external watcher), and refers to
//!    `glimpse-shell` instead of the retired `glimpse-applet`.

use anyhow::{Context, Result, bail};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

fn applets_dir() -> PathBuf {
    glimpse_core::AppletDirectoryScanner::from_process().user_dir
}

fn config_toml_path() -> PathBuf {
    applets_dir()
        .parent()
        .map(|p| p.join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("config.toml"))
}

/// Resolve `<arg>` to an applet.toml path, or `None` if it isn't a path
/// (caller may then treat the arg as a bare applet id).
fn resolve_applet_toml(arg: Option<&str>) -> Option<PathBuf> {
    let p = PathBuf::from(arg.unwrap_or("."));
    let candidate = if p.extension().and_then(|e| e.to_str()) == Some("toml") && !p.is_dir() {
        p
    } else {
        p.join("applet.toml")
    };
    candidate
        .exists()
        .then(|| candidate.canonicalize().unwrap_or(candidate.clone()))
}

/// Parse + validate an applet.toml. Returns `(id, kind)` where kind is
/// "exec" or "command". Rejects the same shapes discovery would silently
/// drop, so `link` can fail loudly.
fn validate_applet_toml(path: &Path) -> Result<(String, &'static str)> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let value: toml::Value =
        toml::from_str(&content).with_context(|| format!("parse {}", path.display()))?;
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .with_context(|| format!("{} has no non-empty `id`", path.display()))?
        .to_owned();
    let kind = match value.get("type").and_then(|v| v.as_str()) {
        Some("command") => "command",
        Some("exec") => "exec",
        Some(other) => bail!(
            "{} has unknown type {other:?} (expected exec or command)",
            path.display()
        ),
        None => bail!(
            "{} has no `type` (expected type = \"exec\" or \"command\")",
            path.display()
        ),
    };
    if value.get(kind).and_then(|v| v.as_table()).is_none() {
        bail!(
            "{} is type={kind} but has no [{kind}] section",
            path.display()
        );
    }
    Ok((id, kind))
}

pub fn print_link_help() {
    println!("glimpse-shell-applets-link");
    println!("Symlink an applet's applet.toml into the applets directory");
    println!();
    println!("USAGE:");
    println!("    glimpse-shell applets link [OPTIONS] [PATH]");
    println!();
    println!("ARGS:");
    println!("    [PATH]   applet.toml or a directory containing it (default: .)");
    println!();
    println!("OPTIONS:");
    println!("    --force      Replace a conflicting non-symlink file");
    println!("    -h, --help   Print help");
}

pub fn link(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_link_help();
        return Ok(());
    }
    let mut force = false;
    let mut path: Option<String> = None;
    for a in args {
        match a.as_str() {
            "--force" => force = true,
            o if o.starts_with('-') => bail!("unknown option: {o}"),
            p => {
                if path.is_some() {
                    bail!("unexpected extra argument: {p}");
                }
                path = Some(p.to_owned());
            }
        }
    }
    let applet_toml = resolve_applet_toml(path.as_deref())
        .with_context(|| "no applet.toml found (pass a path or run inside an applet project)")?;
    let (id, kind) = validate_applet_toml(&applet_toml)?;

    let dir = applets_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let dest = dir.join(format!("{id}.toml"));

    if dest.is_symlink() {
        if dest.read_link().ok().as_deref() == Some(applet_toml.as_path()) {
            println!(
                "already linked: {} → {}",
                dest.display(),
                applet_toml.display()
            );
            return Ok(());
        }
        std::fs::remove_file(&dest)
            .with_context(|| format!("replace stale symlink {}", dest.display()))?;
    } else if dest.exists() {
        if !force {
            bail!(
                "{} exists and is not a symlink — pass --force to replace it",
                dest.display()
            );
        }
        std::fs::remove_file(&dest).with_context(|| format!("remove {}", dest.display()))?;
    }

    std::os::unix::fs::symlink(&applet_toml, &dest)
        .with_context(|| format!("create symlink {}", dest.display()))?;
    println!(
        "linked ({kind}): {} → {}",
        dest.display(),
        applet_toml.display()
    );
    println!(
        "add \"{id}\" to a panel's left/center/right in {} to show it in the bar",
        config_toml_path().display()
    );
    Ok(())
}

pub fn print_unlink_help() {
    println!("glimpse-shell-applets-unlink");
    println!("Remove a symlink created by `applets link`");
    println!();
    println!("USAGE:");
    println!("    glimpse-shell applets unlink [PATH | <id>]");
    println!();
    println!("ARGS:");
    println!("    [PATH | <id>]   project path/applet.toml, or a bare applet id");
    println!("                    (default: applet.toml in the current directory)");
}

pub fn unlink(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_unlink_help();
        return Ok(());
    }
    let arg = args.first().map(String::as_str);
    // A path → read its id; otherwise treat the arg as a bare id so an applet
    // can be unlinked even if its project directory is gone.
    let id = match resolve_applet_toml(arg) {
        Some(toml) => validate_applet_toml(&toml)?.0,
        None => match arg {
            Some(a) if !a.is_empty() && !a.starts_with('-') => a.to_owned(),
            _ => bail!("no applet.toml found and no id given"),
        },
    };
    let dest = applets_dir().join(format!("{id}.toml"));
    if !dest.is_symlink() {
        if dest.exists() {
            bail!(
                "{} is not a symlink — refusing to remove it",
                dest.display()
            );
        }
        println!("{id} is not linked (no {})", dest.display());
        return Ok(());
    }
    std::fs::remove_file(&dest).with_context(|| format!("remove {}", dest.display()))?;
    println!("unlinked: {}", dest.display());
    Ok(())
}

// ── doctor ────────────────────────────────────────────────────────────────────

pub fn print_doctor_help() {
    println!("glimpse-shell-applets-doctor");
    println!("Verify the environment for building and running applets");
    println!();
    println!("USAGE:");
    println!("    glimpse-shell applets doctor [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    --lang <rust|python|typescript|go>   Only probe one language");
    println!("    --strict                             Exit non-zero if any required check fails");
    println!("    -h, --help                           Print help");
}

enum St {
    Ok(String),
    Missing,
    Optional,
}

struct Doc {
    ok: usize,
    missing: usize,
    optional: usize,
}

fn report(doc: &mut Doc, name: &str, st: St, hint: Option<&str>) {
    match st {
        St::Ok(v) => {
            doc.ok += 1;
            println!("  ✓ {name} ({v})");
        }
        St::Optional => {
            doc.optional += 1;
            println!("  - {name} (optional, not found)");
        }
        St::Missing => {
            doc.missing += 1;
            println!("  ✗ {name} (not found)");
            if let Some(h) = hint {
                println!("      → {h}");
            }
        }
    }
}

/// Run `cmd <args>`; first output line = version. `NotFound` = absent
/// (no `which` dependency).
fn probe(cmd: &str, args: &[&str]) -> Option<String> {
    match Command::new(cmd).args(args).output() {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout);
            let e = String::from_utf8_lossy(&out.stderr);
            Some(
                s.lines()
                    .next()
                    .or_else(|| e.lines().next())
                    .map(str::trim)
                    .unwrap_or("present")
                    .to_owned(),
            )
        }
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(_) => Some("present".to_owned()),
    }
}

fn tool(doc: &mut Doc, cmd: &str, args: &[&str], required: bool, hint: &str) {
    match probe(cmd, args) {
        Some(v) => report(doc, cmd, St::Ok(v), None),
        None if required => report(doc, cmd, St::Missing, Some(hint)),
        None => report(doc, cmd, St::Optional, None),
    }
}

pub fn doctor(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_doctor_help();
        return Ok(());
    }
    let mut only: Option<String> = None;
    let mut strict = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--lang" => {
                let v = it.next().context("--lang requires a value")?.clone();
                if !matches!(v.as_str(), "rust" | "python" | "typescript" | "go") {
                    bail!("unknown language {v:?} (expected rust, python, typescript or go)");
                }
                only = Some(v);
            }
            "--strict" => strict = true,
            o if o.starts_with('-') => bail!("unknown option: {o}"),
            p => bail!("unexpected argument: {p}"),
        }
    }

    println!("Glimpse applet environment check");
    println!();
    let mut doc = Doc {
        ok: 0,
        missing: 0,
        optional: 0,
    };

    // Glimpse itself + the dirs the applets workflow needs.
    tool(
        &mut doc,
        "glimpse-shell",
        &["--version"],
        true,
        "install Glimpse — https://github.com/alex-oleshkevich/glimpse",
    );
    let tdir = std::env::var_os("GLIMPSE_APPLET_TEMPLATES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/share/glimpse/applet-templates"));
    if tdir.join("command").is_dir() && tdir.join("exec").is_dir() {
        report(
            &mut doc,
            "applet-templates",
            St::Ok(tdir.display().to_string()),
            None,
        );
    } else {
        report(
            &mut doc,
            "applet-templates",
            St::Missing,
            Some(
                "needed by `applets new`; install the package or set GLIMPSE_APPLET_TEMPLATES_DIR",
            ),
        );
    }
    let adir = applets_dir();
    match std::fs::create_dir_all(&adir) {
        Ok(()) => report(
            &mut doc,
            "applets-dir",
            St::Ok(adir.display().to_string()),
            None,
        ),
        Err(e) => report(&mut doc, "applets-dir", St::Missing, Some(&format!("{e}"))),
    }

    let langs: &[(&str, &[(&str, &[&str], bool, &str)])] = &[
        (
            "rust",
            &[
                (
                    "cargo",
                    &["--version"],
                    true,
                    "Rust via rustup → https://rustup.rs",
                ),
                (
                    "rustc",
                    &["--version"],
                    true,
                    "Rust via rustup → https://rustup.rs",
                ),
            ],
        ),
        (
            "python",
            &[
                (
                    "python",
                    &["--version"],
                    true,
                    "Python 3.14+ from your distro",
                ),
                ("uv", &["--version"], false, ""),
            ],
        ),
        (
            "typescript",
            &[
                (
                    "node",
                    &["--version"],
                    true,
                    "Node 20+ → https://nodejs.org",
                ),
                ("npm", &["--version"], true, "Node 20+ ships npm"),
            ],
        ),
        (
            "go",
            &[("go", &["version"], true, "Go 1.24+ → https://go.dev/dl")],
        ),
    ];
    for (name, checks) in langs {
        if let Some(want) = &only {
            if want != name {
                continue;
            }
        }
        for (cmd, vargs, required, hint) in *checks {
            tool(&mut doc, cmd, vargs, *required, hint);
        }
    }

    println!();
    println!(
        "Summary: {} ok, {} optional missing, {} missing.",
        doc.ok, doc.optional, doc.missing
    );
    if strict && doc.missing > 0 {
        bail!("{} required checks failed", doc.missing);
    }
    Ok(())
}
