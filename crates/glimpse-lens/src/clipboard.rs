use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use glimpse_dbus::clipboard::Clipboard1Proxy;
use zbus::proxy::CacheProperties;

const TIMEOUT: Duration = Duration::from_secs(3);

pub fn copy(text: &str) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("cannot start the clipboard runtime")?;
    runtime
        .block_on(async {
            match tokio::time::timeout(TIMEOUT, send(text)).await {
                Ok(sent) => sent,
                Err(_) => Err(anyhow!("the panel did not answer in time")),
            }
        })
        .context("the value could not be copied")
}

async fn send(text: &str) -> Result<()> {
    let connection = zbus::Connection::session()
        .await
        .context("cannot reach the session bus")?;
    let proxy = zbus::proxy::Builder::<Clipboard1Proxy>::new(&connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .context("cannot reach the panel clipboard")?;
    proxy
        .copy_text(text)
        .await
        .context("cannot send the text to the panel")
}
