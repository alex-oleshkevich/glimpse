use crate::daemon::Daemon;
use tokio::signal;

mod daemon;
mod messages;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let client_socket_path = dirs::runtime_dir()
        .map(|d| d.join("glimpsed.sock"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/glimpsed.sock"));
    let plugin_socket_path = dirs::runtime_dir()
        .map(|d| d.join("glimpsed-plugins.sock"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/glimpsed-plugins.sock"));

    let daemon = Daemon::new(client_socket_path, plugin_socket_path);
    let daemon_handle = tokio::spawn(async move {
        if let Err(e) = daemon.run().await {
            tracing::error!("error in daemon: {}", e);
        }
    });

    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
    let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())?;

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::debug!("received SIGTERM, shutting down gracefully");
        },
        _ = sigint.recv() => {
            tracing::debug!("received SIGINT, shutting down gracefully");
        },
        _ = daemon_handle => {
            tracing::debug!("daemon finished");
        }
    }

    Ok(())
}
