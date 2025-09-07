use crate::daemon::Daemon;
use tokio::signal;
mod daemon;
mod plugins;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let daemon = Daemon::new().discover_plugins()?.start_plugins()?;

    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
    let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())?;

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::debug!("received SIGTERM, shutting down gracefully");
        },
        _ = sigint.recv() => {
            tracing::debug!("received SIGINT, shutting down gracefully");
        },
        _ = daemon.run() => {
            tracing::debug!("daemon finished");
        }
    }

    Ok(())
}
