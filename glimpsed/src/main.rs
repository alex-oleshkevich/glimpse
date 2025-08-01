use crate::{
    messages::{Message, MessageBus},
    plugin_host::PluginHost,
    rpc_host::RPCHost,
};
use glimpse_sdk::{JSONRPCRequest, Request};
use tokio::signal;

mod messages;
mod plugin_host;
mod rpc_host;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let message_bus = MessageBus::new();
    let rpc_host = RPCHost::new(&message_bus);
    let host = PluginHost::new(&message_bus);

    let client_handle = tokio::spawn(async move {
        if let Err(e) = rpc_host.run().await {
            tracing::error!("error in RPC host: {}", e);
        }
    });
    let plugin_handle = tokio::spawn(async move {
        if let Err(e) = host.run().await {
            tracing::error!("error in plugin host: {}", e)
        }
    });

    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
    let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())?;

    tokio::select! {
    _ = sigterm.recv() => {
        let message = Message::ClientRequest(
            JSONRPCRequest::<Request> { jsonrpc: "2.0".to_string(), method: "quit".to_string(), params: Some(Request::Quit), id: serde_json::Value::Null }
        );
        if let Err(e)= message_bus.publisher().send(message) {
            tracing::error!("error sending quit message: {}", e);
        }
        tracing::info!("received SIGTERM, shutting down gracefully");
    },
    _ = sigint.recv() => {
        let message = Message::ClientRequest(
            JSONRPCRequest::<Request> { jsonrpc: "2.0".to_string(), method: "quit".to_string(), params: Some(Request::Quit), id: serde_json::Value::Null }
        );
        if let Err(e)= message_bus.publisher().send(message) {
            tracing::error!("error sending quit message: {}", e);
        }
        tracing::info!("received SIGINT, shutting down gracefully");
    },
    _ = client_handle => {
        tracing::info!("rpc server finished");
    },
    _ = plugin_handle => {
        tracing::info!("plugin host finished");
    }
    }

    Ok(())
}
