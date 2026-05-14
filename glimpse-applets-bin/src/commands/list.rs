use anyhow::Result;
use clap::Args as ClapArgs;
use std::fs;

use crate::commands::config::glimpse_config_dir;

#[derive(ClapArgs)]
pub struct Args {}

pub fn run(_args: Args) -> Result<()> {
    let applets_dir = glimpse_config_dir().join("applets");

    if !applets_dir.exists() {
        println!("no applets installed");
        return Ok(());
    }

    let mut entries: Vec<(String, &'static str, Option<String>)> = vec![];

    for entry in fs::read_dir(&applets_dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".toml") {
            continue;
        }

        let (id, kind) = if let Some(stem) = name.strip_suffix(".dev.toml") {
            (stem.to_string(), "dev")
        } else {
            (name.strip_suffix(".toml").unwrap().to_string(), "linked")
        };

        let target = if path.is_symlink() {
            fs::read_link(&path).ok().map(|t| t.display().to_string())
        } else {
            None
        };

        entries.push((id, kind, target));
    }

    if entries.is_empty() {
        println!("no applets installed");
        return Ok(());
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let id_w = entries.iter().map(|(id, _, _)| id.len()).max().unwrap_or(0);
    let kind_w = entries.iter().map(|(_, k, _)| k.len()).max().unwrap_or(0);

    for (id, kind, target) in &entries {
        match target {
            Some(t) => println!("{id:<id_w$}  {kind:<kind_w$}  → {t}"),
            None => println!("{id:<id_w$}  {kind:<kind_w$}"),
        }
    }

    Ok(())
}
