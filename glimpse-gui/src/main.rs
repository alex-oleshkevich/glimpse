mod app;
mod messages;
use tracing_subscriber::{EnvFilter, prelude::*};

use anyhow;
use glimpse_sdk::{JSONRPCRequest, JSONRPCResponse, get_client_socket_path};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    sync::mpsc,
};

use crate::{app::App, messages::Message};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    setup_logging();
    let socket_path = get_client_socket_path();

    let stream = tokio::net::UnixStream::connect(&socket_path).await;
    if stream.is_err() {
        return Err(anyhow::anyhow!("failed to connect to socket"));
    }
    let stream = stream.unwrap();
    let (reader, writer) = tokio::io::split(stream);
    let mut writer = writer;
    let mut reader = tokio::io::BufReader::new(reader);

    let (from_daemon_tx, from_daemon_rx) = mpsc::channel::<Message>(16);
    let (to_daemon_tx, mut to_daemon_rx) = mpsc::channel::<Message>(16);

    // reader
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let rpc_message = serde_json::from_str::<JSONRPCResponse>(&line);
                    if rpc_message.is_err() {
                        tracing::error!("failed to parse JSON-RPC message: {}", line);
                        line.clear();
                        continue;
                    }

                    let rpc_message = rpc_message.unwrap();
                    let response = rpc_message.result;
                    match response {
                        _ => {
                            tracing::debug!("received message from daemon: {:?}", response);
                            from_daemon_tx
                                .send(Message::DaemonResponse {
                                    request_id: rpc_message.id,
                                    plugin_id: rpc_message.plugin_id,
                                    response: response,
                                })
                                .await
                                .ok();
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("failed to read line: {}", e);
                    break;
                }
            }
            line.clear();
        }
    });

    // writer
    tokio::spawn(async move {
        while let Some(message) = to_daemon_rx.recv().await {
            match message {
                Message::DispatchRequest(request) => {
                    let rpc_request = JSONRPCRequest::new(request);
                    if let Ok(response_str) = rpc_request.to_string() {
                        if writer.write_all(response_str.as_bytes()).await.is_err() {
                            tracing::error!("failed to write response to socket");
                        }
                    } else {
                        tracing::error!("failed to serialize response");
                    }
                }
                _ => {}
            }
        }
    });

    let daemon = iced::daemon("Glimpse", App::update, App::view)
        .subscription(App::subscription)
        .run_with(|| App::new(from_daemon_rx, to_daemon_tx));

    if daemon.is_err() {
        tracing::error!("failed to run daemon: {}", daemon.unwrap_err());
        return Err(anyhow::anyhow!("failed to run daemon"));
    }

    Ok(())
}

fn setup_logging() {
    let filter = EnvFilter::new("warn")
        .add_directive("glimpse_gui=debug".parse().unwrap())
        .add_directive("glimpse_sdk=debug".parse().unwrap())
        .add_directive("glimpsed=debug".parse().unwrap());

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_max_level(tracing::Level::DEBUG)
        .init();
}
