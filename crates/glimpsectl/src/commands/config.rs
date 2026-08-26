use std::path::PathBuf;

use anyhow::Result;

use crate::render::{Flow, print};

pub fn config_show(override_path: Option<PathBuf>) -> Result<()> {
    let config = glimpse_config::load(override_path.as_deref())?;
    print(toml::to_string_pretty(&config)?.trim_end())?;
    Ok(())
}

pub fn config_validate(override_path: Option<PathBuf>) -> Result<()> {
    glimpse_config::load(override_path.as_deref())?;
    Ok(())
}

pub fn config_path(config: Option<PathBuf>) -> Result<()> {
    for path in glimpse_config::resolved_files(config.as_deref())? {
        if let Flow::Stop = print(&path.display().to_string())? {
            break;
        }
    }
    Ok(())
}
