use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use glimpse_config::{DATA_DIR, is_desktop_id};
use glimpse_services::DesktopCatalog;
use minijinja::{AutoEscape, Environment, UndefinedBehavior, context};

use crate::cli::NewAppletArgs;
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
            "cd {}\nnpm install\nglimpsectl applets run --dev .",
            shown.display()
        ))
        .print()?;
    println!();
    Section::new("config.toml")
        .with(format!("[applets.\"{id}\"]\napplet = \"{id}\""))
        .with_note(&format!("then add it to a panel zone: center = [\"{id}\"]"))
        .print()
}
