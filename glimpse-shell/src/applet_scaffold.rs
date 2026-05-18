//! `glimpse-shell applets new` — scaffold a new applet project from the
//! shipped templates. Templates live on disk (not embedded) under
//! `$GLIMPSE_APPLET_TEMPLATES_DIR` or `/usr/share/glimpse/applet-templates`,
//! laid out as `<dir>/{command,exec}/applet.toml` and
//! `<dir>/exec/<lang>/...` language scaffolds.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

const SYSTEM_TEMPLATES_DIR: &str = "/usr/share/glimpse/applet-templates";
const TEMPLATES_DIR_ENV: &str = "GLIMPSE_APPLET_TEMPLATES_DIR";

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppletKind {
    Exec,
    Command,
}

impl AppletKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Exec => "exec",
            Self::Command => "command",
        }
    }
}

#[derive(Clone, Copy)]
enum Language {
    Rust,
    Python,
    Typescript,
    Go,
}

impl Language {
    fn dir(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Typescript => "typescript",
            Self::Go => "go",
        }
    }
}

fn templates_dir() -> PathBuf {
    std::env::var_os(TEMPLATES_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(SYSTEM_TEMPLATES_DIR))
}

pub fn print_help() {
    println!("glimpse-shell-applets-new");
    println!("Scaffold a new applet project from the shipped templates");
    println!();
    println!("USAGE:");
    println!("    glimpse-shell applets new <NAME> [OPTIONS]");
    println!();
    println!("ARGS:");
    println!("    <NAME>   Project name (directory name; [A-Za-z0-9_-])");
    println!();
    println!("OPTIONS:");
    println!("    --lang <rust|python|typescript|go>   Language (default: rust; exec only)");
    println!("    --type <exec|command>                Applet type (default: exec)");
    println!("    --dir <DIR>                          Parent directory (default: .)");
    println!("    --force                              Scaffold into a non-empty directory");
    println!("    -h, --help                           Print help");
}

/// Parse `applets new` argv (everything after `new`). Manual parsing to match
/// the shell binary's clap-free CLI style.
pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }

    let mut name: Option<String> = None;
    let mut lang = Language::Rust;
    let mut kind = AppletKind::Exec;
    let mut dir: Option<PathBuf> = None;
    let mut force = false;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--lang" => {
                lang = match it.next().map(String::as_str) {
                    Some("rust") => Language::Rust,
                    Some("python") => Language::Python,
                    Some("typescript") => Language::Typescript,
                    Some("go") => Language::Go,
                    other => bail!(
                        "--lang must be rust|python|typescript|go, got {:?}",
                        other.unwrap_or("")
                    ),
                };
            }
            "--type" => {
                kind = match it.next().map(String::as_str) {
                    Some("exec") => AppletKind::Exec,
                    Some("command") => AppletKind::Command,
                    other => bail!("--type must be exec|command, got {:?}", other.unwrap_or("")),
                };
            }
            "--dir" => {
                dir = Some(PathBuf::from(it.next().context("--dir requires a value")?));
            }
            "--force" => force = true,
            other if other.starts_with('-') => bail!("unknown option: {other}"),
            positional => {
                if name.is_some() {
                    bail!("unexpected extra argument: {positional}");
                }
                name = Some(positional.to_owned());
            }
        }
    }

    let name = name.context("applet name is required (glimpse-shell applets new <NAME>)")?;
    validate_name(&name)?;

    let tpl = templates_dir();
    if !tpl.is_dir() {
        bail!(
            "applet templates not found at {} (set {} to override)",
            tpl.display(),
            TEMPLATES_DIR_ENV
        );
    }

    let parent = dir.unwrap_or_else(|| PathBuf::from("."));
    let project_dir = parent.join(&name);

    if project_dir.exists() && !force {
        let empty = project_dir.is_dir()
            && fs::read_dir(&project_dir)
                .map(|mut d| d.next().is_none())
                .unwrap_or(false);
        if !empty {
            bail!(
                "{} already exists. pass --force to scaffold into it anyway.",
                project_dir.display()
            );
        }
    }

    fs::create_dir_all(&project_dir)
        .with_context(|| format!("failed to create {}", project_dir.display()))?;

    if kind == AppletKind::Exec {
        let lang_root = tpl.join("exec").join(lang.dir());
        copy_rendered_tree(&lang_root, &project_dir, &name)
            .with_context(|| format!("copying {} template", lang.dir()))?;
    }

    let applet_toml_src = tpl.join(kind.as_str()).join("applet.toml");
    let applet_toml = render(
        &fs::read_to_string(&applet_toml_src)
            .with_context(|| format!("read {}", applet_toml_src.display()))?,
        &name,
    );
    fs::write(project_dir.join("applet.toml"), applet_toml)
        .with_context(|| format!("write {}/applet.toml", project_dir.display()))?;

    print_next_steps(kind, lang, &name, &project_dir);
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("project name must not be empty");
    }
    if name.starts_with('-') || name.starts_with('.') {
        bail!("project name must not start with '-' or '.'");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("project name {name:?} contains characters other than [A-Za-z0-9_-]");
    }
    Ok(())
}

fn render(template: &str, name: &str) -> String {
    template
        .replace("__NAME__", name)
        .replace("__NAME_PY__", &name.replace('-', "_"))
}

/// Recursively copy every file under `src` into `dst`, rendering placeholders.
/// Copying the whole template dir (rather than a hardcoded file list) keeps
/// the scaffolder and the on-disk templates in sync automatically.
fn copy_rendered_tree(src: &Path, dst: &Path, name: &str) -> Result<()> {
    for entry in fs::read_dir(src).with_context(|| format!("read dir {}", src.display()))? {
        let entry = entry?;
        let from = entry.path();
        let rendered_name = render(&entry.file_name().to_string_lossy(), name);
        let to = dst.join(&rendered_name);
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&to)?;
            copy_rendered_tree(&from, &to, name)?;
        } else {
            let body = render(
                &fs::read_to_string(&from).with_context(|| format!("read {}", from.display()))?,
                name,
            );
            if let Some(p) = to.parent() {
                fs::create_dir_all(p)?;
            }
            fs::write(&to, body).with_context(|| format!("write {}", to.display()))?;
        }
    }
    Ok(())
}

fn print_next_steps(kind: AppletKind, lang: Language, name: &str, project_dir: &Path) {
    let absolute = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    println!(
        "Created {} ({} applet)",
        project_dir.display(),
        kind.as_str()
    );
    println!();
    println!("Edit applet.toml to configure the applet.");
    println!();
    println!("Next steps:");
    println!("  cd {}", project_dir.display());
    match kind {
        AppletKind::Command => {
            println!();
            println!("Fill in icon and click commands in applet.toml, then link it:");
            println!("  glimpse-shell applets link");
        }
        AppletKind::Exec => {
            println!();
            if matches!(lang, Language::Python) {
                let module = name.replace('-', "_");
                println!("Start the applet:");
                println!("  uv run python -m {module}");
                println!();
                println!("Set command in applet.toml:");
                println!("  command = [\"uv\", \"run\", \"python\", \"-m\", \"{module}\"]");
                println!();
                println!("Then link it:");
            } else {
                println!("Build the program, set `command = [...]` in applet.toml to its");
                println!("binary/entrypoint, then link it:");
            }
            println!("  glimpse-shell applets link");
            println!("or run the dev server for live-reload:");
            println!("  glimpse-shell applets dev {}", absolute.display());
        }
    }
}
