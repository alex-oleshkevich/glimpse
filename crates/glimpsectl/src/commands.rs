use std::path::PathBuf;

use anyhow::Result;
use glimpse_ipc::Client;

pub async fn get(client: &Client, topic: String, field: Option<String>, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn watch(client: &Client, pattern: String, count: Option<u64>, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn call(
    client: &Client,
    method: String,
    args: Vec<(String, String)>,
    json: bool,
) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn topics(client: &Client, pattern: Option<String>, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn services(client: &Client, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn config_show(client: &Client, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn doctor(client: &Client, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub async fn monitor(client: &Client) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub fn config_validate(path: Option<PathBuf>, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}

pub fn config_path(config: Option<PathBuf>, json: bool) -> Result<()> {
    anyhow::bail!("not implemented")
}
