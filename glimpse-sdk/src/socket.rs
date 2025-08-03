use dirs;
use std::path::PathBuf;
use tokio::net::{UnixListener, UnixStream};

pub fn get_client_socket_path() -> PathBuf {
    dirs::runtime_dir()
        .map(|d| d.join("glimpsed.sock"))
        .unwrap_or_else(|| PathBuf::from("/tmp/glimpsed.sock"))
}

pub fn get_plugin_socket_path() -> PathBuf {
    dirs::runtime_dir()
        .map(|d| d.join("glimpsed-plugins.sock"))
        .unwrap_or_else(|| PathBuf::from("/tmp/glimpsed-plugins.sock"))
}

pub async fn safe_bind(path: &std::path::PathBuf) -> anyhow::Result<UnixListener> {
    match UnixListener::bind(path) {
        Ok(listener) => Ok(listener),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            match UnixStream::connect(path).await {
                Ok(_) => Err(anyhow::anyhow!(
                    "application is already running, socket in use"
                )),
                Err(_) => {
                    std::fs::remove_file(path)?;
                    Ok(UnixListener::bind(path)?)
                }
            }
        }
        Err(e) => Err(anyhow::anyhow!(
            "failed to bind to socket at {}: {}",
            path.display(),
            e
        )),
    }
}
