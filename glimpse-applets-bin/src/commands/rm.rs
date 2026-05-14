use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::commands::config::glimpse_config_dir;

#[derive(ClapArgs)]
pub struct Args {
    /// Applet ID to remove.
    id: String,
    /// Skip confirmation prompt.
    #[arg(short, long)]
    yes: bool,
}

pub fn run(args: Args) -> Result<()> {
    let applets_dir = glimpse_config_dir().join("applets");
    let path = applets_dir.join(format!("{}.toml", args.id));

    if !path.exists() && !path.is_symlink() {
        bail!("applet \"{}\" is not installed", args.id);
    }

    let description = describe(&path);

    if !args.yes {
        print!("remove {description}? [y/N] ");
        io::stdout().flush().context("flush stdout")?;
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("read confirmation")?;
        if !matches!(input.trim(), "y" | "Y") {
            println!("aborted");
            return Ok(());
        }
    }

    std::fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    println!("removed {description}");
    Ok(())
}

fn describe(path: &PathBuf) -> String {
    if path.is_symlink() {
        let target = std::fs::read_link(path)
            .map(|t| format!(" → {}", t.display()))
            .unwrap_or_default();
        format!("\"{}\" (linked{target})", stem(path))
    } else {
        format!("\"{}\"", stem(path))
    }
}

fn stem(path: &PathBuf) -> &str {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("")
}
