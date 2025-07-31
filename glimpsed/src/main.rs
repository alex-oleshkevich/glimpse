use crate::{plugin_host::PluginHost, rpc_host::RPCHost};
use tokio::signal;

mod plugin_host;
mod rpc_host;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let (plugin_msg_tx, plugin_msg_rx) = tokio::sync::mpsc::channel::<String>(100);
    let (plugin_host_tx, plugin_host_rx) = tokio::sync::mpsc::channel::<String>(100);

    let (rpc_msg_tx, rpc_msg_rx) = tokio::sync::mpsc::channel::<String>(100);
    let (rpc_host_tx, rpc_host_rx) = tokio::sync::mpsc::channel::<String>(100);

    let client_handle = tokio::spawn(async move {
        let rpc_host = RPCHost::new(rpc_host_rx, rpc_msg_tx);
        if let Err(e) = rpc_host.run().await {
            tracing::error!("error in RPC host: {}", e);
        }
    });

    let plugin_host_tx_clone = plugin_host_tx.clone();
    let client_message_handler = tokio::spawn(async move {
        let mut rx = rpc_msg_rx;
        while let Some(msg) = rx.recv().await {
            tracing::info!("relay client message to plugins: {}", msg);
            plugin_host_tx_clone.send(msg).await.unwrap();
        }
    });

    let plugin_handle = tokio::spawn(async move {
        let host = PluginHost::new(plugin_host_rx, plugin_msg_tx);
        if let Err(e) = host.run().await {
            tracing::error!("error in plugin host: {}", e)
        }
    });

    let plugin_message_handler = tokio::spawn(async move {
        let mut rx = plugin_msg_rx;
        while let Some(msg) = rx.recv().await {
            tracing::info!("received message from plugin: {}", msg);
            if let Err(e) = rpc_host_tx.send(msg).await {
                tracing::error!("failed to send message to RPC host: {}", e);
            }
        }
    });

    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
    let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())?;

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::info!("received SIGTERM, shutting down gracefully");
        },
        _ = sigint.recv() => {
            tracing::info!("received SIGINT, shutting down gracefully");
        },
        _ = client_handle => {
            tracing::info!("rpc server finished");
        },
        _ = plugin_handle => {
            tracing::info!("plugin host finished");
        },
        _ = plugin_message_handler => {
            tracing::info!("plugin message handler finished");
        },
        _ = client_message_handler => {
            tracing::info!("client message handler finished");
        },
    }

    Ok(())
}
