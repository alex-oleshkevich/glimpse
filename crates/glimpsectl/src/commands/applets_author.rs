use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use glimpse_config::{DATA_DIR, is_desktop_id};
use glimpse_services::DesktopCatalog;
use minijinja::{AutoEscape, Environment, UndefinedBehavior, context};

use crate::cli::{AppletLanguage, NewAppletArgs};

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

pub(super) fn slug(id: &str) -> String {
    let mut slug = String::new();
    for ch in words(id).1.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }

    let slug = slug.trim_matches('-');
    if slug
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic())
    {
        slug.to_owned()
    } else if slug.is_empty() {
        "applet".to_owned()
    } else {
        format!("applet-{slug}")
    }
}

fn write_file(path: &Path, content: &[u8]) -> Result<()> {
    fs::write(path, content)?;
    if content.starts_with(b"#!") {
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn template_dir(language: AppletLanguage, templates_root: Option<PathBuf>) -> PathBuf {
    templates_root
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new(DATA_DIR).join("sdk/templates"))
        .join(language.dir())
}

fn collect_template_files(dir: &Path, root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("applet template not installed at {}", root.display()))?
        .collect::<io::Result<_>>()?;

    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        ensure!(
            !file_type.is_symlink(),
            "template contains a symlink: {}",
            path.display()
        );
        if file_type.is_dir() {
            collect_template_files(&path, root, files)?;
        } else if file_type.is_file() {
            files.push(path.strip_prefix(root)?.to_owned());
        } else {
            bail!("template contains a non-regular file: {}", path.display());
        }
    }
    Ok(())
}

pub fn applets_new(args: NewAppletArgs, json: bool) -> Result<()> {
    let template = template_dir(
        args.language,
        env::var_os("GLIMPSE_SDK_TEMPLATES_DIR").map(PathBuf::from),
    );
    ensure!(
        template.is_dir(),
        "applet template not installed at {}",
        template.display()
    );
    let installed = DesktopCatalog.installed();
    new_with_catalog(
        args,
        &template,
        |id| installed.iter().any(|(name, _)| name == id),
        json,
    )
}

pub fn applets_new_with_catalog(
    args: NewAppletArgs,
    template: &Path,
    installed: impl Fn(&str) -> bool,
) -> Result<()> {
    new_with_catalog(args, template, installed, false)
}

fn new_with_catalog(
    args: NewAppletArgs,
    template: &Path,
    installed: impl Fn(&str) -> bool,
    json: bool,
) -> Result<()> {
    ensure!(
        template.is_dir(),
        "applet template not installed at {}",
        template.display()
    );
    let language = args.language;
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
    let slug = slug(&id);

    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| AutoEscape::None);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_keep_trailing_newline(true);

    let context = context! { id, name, description, icon, dir, slug, language => language.dir() };
    let mut relative_files = Vec::new();
    collect_template_files(template, template, &mut relative_files)?;

    let mut outputs = HashSet::new();
    let mut planned = Vec::new();

    for relative in relative_files {
        let source = template.join(&relative);
        let relative_text = relative
            .to_str()
            .context("template file name is not UTF-8")?;
        let rendered = relative_text.strip_suffix(".jinja").is_some();
        let output_text = relative_text
            .strip_suffix(".jinja")
            .unwrap_or(relative_text);
        let output_text = env.render_str(output_text, &context)?;
        let output = PathBuf::from(&output_text);

        ensure!(
            !output.as_os_str().is_empty()
                && output
                    .components()
                    .all(|component| matches!(component, Component::Normal(_))),
            "template path escapes destination: {}",
            relative.display()
        );
        ensure!(
            outputs.insert(output.clone()),
            "template files render to the same path: {}",
            output.display()
        );

        let content = if rendered {
            env.render_str(&fs::read_to_string(&source)?, &context)
                .with_context(|| format!("cannot render {}", relative.display()))?
                .into_bytes()
        } else {
            fs::read(&source)?
        };

        planned.push((output, content));
    }
    for (relative, content) in planned {
        let output = dir.join(relative);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }

        write_file(&output, &content)?;
    }
    let shown = env::current_dir()
        .ok()
        .and_then(|cwd| dir.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .filter(|relative| !relative.as_os_str().is_empty())
        .unwrap_or_else(|| dir.clone());
    if json {
        super::emit(&serde_json::json!({"id": id, "language": language.dir(), "path": dir}))?;
    } else {
        crate::render::print(&id)?;
    }
    eprintln!(
        "Created {id} in {}\n\ncd {}\n{}\nglimpsectl applet run --dev --isolated .\n\n[applets.\"{id}\"]\napplet = \"{id}\"\nthen add it to a panel zone: center = [\"{id}\"]",
        shown.display(),
        shown.display(),
        language.setup_hint()
    );
    Ok(())
}

#[cfg(test)]
mod tests;
