use std::path::PathBuf;

use glimpse_config::Documents;

use anyhow::Result;

use super::emit;
use crate::render::{Flow, print, styled};

pub fn config_show(json: bool) -> Result<()> {
    let config = glimpse_config::load(Documents::ConfigAndPanel)?;
    match json {
        true => emit(&config),
        false => {
            print(toml::to_string_pretty(&config)?.trim_end())?;
            Ok(())
        }
    }
}

pub fn config_validate(path: Option<PathBuf>) -> Result<()> {
    config_validate_output(path, false)
}

pub fn config_validate_output(path: Option<PathBuf>, json: bool) -> Result<()> {
    let config = match path {
        Some(path) => {
            anyhow::ensure!(path.exists(), "{}: no such file", path.display());
            glimpse_config::load_files(&[path])?
        }
        None => glimpse_config::load(Documents::ConfigAndPanel)?,
    };
    let problems: Vec<String> = config
        .validate()
        .into_iter()
        .map(|problem| problem.to_string())
        .collect();
    if json {
        emit(&serde_json::json!({"valid": problems.is_empty(), "problems": problems}))?;
    } else {
        for problem in &problems {
            if let Flow::Stop = print(&styled::warn(problem))? {
                break;
            }
        }
    }
    anyhow::ensure!(problems.is_empty(), "configuration validation failed");
    Ok(())
}

pub fn config_path(json: bool) -> Result<()> {
    let files = glimpse_config::config_files(Documents::ConfigAndPanel)?;
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
