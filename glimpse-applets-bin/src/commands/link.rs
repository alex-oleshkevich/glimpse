use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use std::path::PathBuf;

use crate::commands::config::glimpse_config_dir;

#[derive(ClapArgs)]
pub struct Args {
    /// Path to applet.toml or a project directory containing it. Defaults to the current directory.
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    let applet_toml = resolve_applet_toml(args.path)?;
    if !applet_toml.exists() {
        bail!("no applet.toml found at {}", applet_toml.display());
    }
    let id = read_id(&applet_toml)?;
    let link_path = link_path_for(&id)?;
    install_link(&applet_toml, &link_path, &id)
}

pub mod unlink {
    use super::*;

    #[derive(ClapArgs)]
    pub struct Args {
        /// Path to applet.toml or a project directory containing it. Defaults to the current directory.
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    }

    pub fn run(args: Args) -> Result<()> {
        let applet_toml = resolve_applet_toml(args.path)?;
        if !applet_toml.exists() {
            bail!("no applet.toml found at {}", applet_toml.display());
        }
        let id = read_id(&applet_toml)?;
        let link_path = link_path_for(&id)?;
        do_unlink(&link_path, &id)
    }
}

fn resolve_applet_toml(path: Option<PathBuf>) -> Result<PathBuf> {
    let p = path.unwrap_or_else(|| PathBuf::from("."));
    // Determine the candidate path before canonicalizing so that a missing
    // file produces the clear "no applet.toml found" error from the caller,
    // not an opaque OS error from canonicalize.
    let candidate = if p.extension().and_then(|e| e.to_str()) == Some("toml") && !p.is_dir() {
        p
    } else {
        p.join("applet.toml")
    };
    // Canonicalize only when the file already exists; otherwise return the
    // raw path so the caller's exists() check fires with a useful message.
    if candidate.exists() {
        candidate.canonicalize().context("resolve applet.toml path")
    } else {
        Ok(candidate)
    }
}

fn link_path_for(id: &str) -> Result<PathBuf> {
    let applets_dir = glimpse_config_dir().join("applets");
    std::fs::create_dir_all(&applets_dir)
        .with_context(|| format!("create applets dir {}", applets_dir.display()))?;
    Ok(applets_dir.join(format!("{id}.toml")))
}

fn install_link(
    applet_toml: &std::path::Path,
    link_path: &std::path::Path,
    id: &str,
) -> Result<()> {
    if link_path.exists() || link_path.is_symlink() {
        let is_symlink = link_path.is_symlink();
        let points_here = link_path
            .read_link()
            .ok()
            .map(|t| t == applet_toml)
            .unwrap_or(false);

        if is_symlink && points_here {
            println!(
                "already linked: {} → {}",
                link_path.display(),
                applet_toml.display()
            );
            return Ok(());
        }

        if !is_symlink {
            bail!(
                "{} exists and is not a symlink — remove it manually to link this applet",
                link_path.display()
            );
        }

        // Stale symlink pointing elsewhere — replace it.
        std::fs::remove_file(link_path)
            .with_context(|| format!("remove stale symlink {}", link_path.display()))?;
    }

    std::os::unix::fs::symlink(applet_toml, link_path)
        .with_context(|| format!("create symlink {}", link_path.display()))?;

    println!(
        "linked: {} → {}",
        link_path.display(),
        applet_toml.display()
    );
    println!("add \"{id}\" to a panel in ~/.config/glimpse/config.toml to show it in the bar");
    Ok(())
}

fn do_unlink(link_path: &std::path::Path, id: &str) -> Result<()> {
    if !link_path.exists() && !link_path.is_symlink() {
        println!("{id} is not linked (no file at {})", link_path.display());
        return Ok(());
    }
    if !link_path.is_symlink() {
        bail!(
            "{} is not a symlink — remove it manually",
            link_path.display()
        );
    }
    std::fs::remove_file(link_path).with_context(|| format!("remove {}", link_path.display()))?;
    println!("unlinked: {}", link_path.display());
    Ok(())
}

fn read_id(applet_toml: &std::path::Path) -> Result<String> {
    let content = std::fs::read_to_string(applet_toml)
        .with_context(|| format!("read {}", applet_toml.display()))?;
    let value: toml::Value =
        toml::from_str(&content).with_context(|| format!("parse {}", applet_toml.display()))?;
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .with_context(|| {
            format!(
                "{} is missing a non-empty `id` field",
                applet_toml.display()
            )
        })?
        .to_string();
    Ok(id)
}
