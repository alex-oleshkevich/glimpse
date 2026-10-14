use std::path::PathBuf;

use anyhow::Result;

use super::emit;
use crate::render::{Flow, print, styled};

pub fn config_show(override_path: Option<PathBuf>, json: bool) -> Result<()> {
    let config = glimpse_config::load(override_path.as_deref())?;
    match json {
        true => emit(&config),
        false => {
            print(toml::to_string_pretty(&config)?.trim_end())?;
            Ok(())
        }
    }
}

pub fn config_validate(override_path: Option<PathBuf>) -> Result<()> {
    let config = glimpse_config::load(override_path.as_deref())?;
    for problem in config.validate() {
        if let Flow::Stop = print(&styled::warn(&problem.to_string()))? {
            break;
        }
    }
    Ok(())
}

pub fn config_path(config: Option<PathBuf>, json: bool) -> Result<()> {
    let files = glimpse_config::resolved_files(config.as_deref())?;
    if json {
        return emit(&files);
    }
    for path in files {
        if let Flow::Stop = print(&path.display().to_string())? {
            break;
        }
    }
    Ok(())
}
