use std::path::PathBuf;

use anyhow::Result;
use glimpse_ipc::Client;

pub async fn get(_client: &Client, _topic: String, _field: Option<String>) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn watch(_client: &Client, _pattern: String, _count: Option<u64>) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn call(_client: &Client, _method: String, _args: Vec<(String, String)>) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn topics(_client: &Client, _pattern: Option<String>) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn services(_client: &Client) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn doctor(_client: &Client) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn monitor(_client: &Client) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub fn config_show(override_path: Option<PathBuf>) -> Result<()> {
    let config = glimpse_config::load(override_path.as_deref())?;
    print!("{}", toml::to_string_pretty(&config)?);
    Ok(())
}

pub fn config_validate(override_path: Option<PathBuf>) -> Result<()> {
    glimpse_config::load(override_path.as_deref())?;
    Ok(())
}

pub fn config_path(config: Option<PathBuf>) -> Result<()> {
    let files = glimpse_config::resolved_files(config.as_deref())?;
    files.into_iter().for_each(|path| println!("{path:?}"));
    Ok(())
}
