use anyhow;
use glimpse_sdk::{JSONRPCRequest, JSONRPCResponse};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::{Mutex, mpsc::UnboundedSender},
};

use crate::messages::Message;

struct ClientConnection {
    client_id: usize,
    writer: UnboundedSender<JSONRPCResponse>,
}

#[derive(Clone)]
struct PluginConnection {
    plugin_id: usize,
    writer: UnboundedSender<JSONRPCRequest>,
}

#[derive(Clone)]
pub struct Daemon {
    client_counter: usize,
    plugin_counter: usize,
    client_socket_path: std::path::PathBuf,
    plugin_socket_path: std::path::PathBuf,
    client_connections: Arc<Mutex<HashMap<usize, ClientConnection>>>,
    plugin_connections: Arc<Mutex<HashMap<usize, PluginConnection>>>,
}

impl Daemon {
    pub fn new(
        client_socket_path: std::path::PathBuf,
        plugin_socket_path: std::path::PathBuf,
    ) -> Self {
        Daemon {
            client_counter: 0,
            plugin_counter: 0,
            client_socket_path,
            plugin_socket_path,
            client_connections: Arc::new(Mutex::new(HashMap::new())),
            plugin_connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        // bind the client socket
        let client_listener = safe_bind(&self.client_socket_path).await?;
        tracing::info!(
            "listening client connections on {}",
            self.client_socket_path.display()
        );

        // bind the plugin socket
        let plugin_listener = safe_bind(&self.plugin_socket_path).await?;
        tracing::info!(
            "listening plugin connections on {}",
            self.plugin_socket_path.display()
        );

        // spawn client listener
        let client_server = self.clone();
        let client_handle = tokio::spawn(async move {
            while let Ok((stream, _)) = client_listener.accept().await {
                let mut client_server = client_server.clone();
                tokio::spawn(async move {
                    tracing::info!("client connected");
                    client_server.handle_client_connection(stream).await;
                });
            }
            tracing::debug!("client listener finished");
        });

        // spawn plugin listener
        let plugin_server = self.clone();
        let plugin_handle = tokio::spawn(async move {
            while let Ok((stream, _)) = plugin_listener.accept().await {
                let mut plugin_server = plugin_server.clone();
                tokio::spawn(async move {
                    tracing::info!("plugin connected");
                    plugin_server.handle_plugin_connection(stream).await;
                });
            }
            tracing::debug!("plugin listener finished");
        });

        tokio::select! {
            _ = client_handle => {
                tracing::debug!("client listener finished");
            },
            _ = plugin_handle => {
                tracing::debug!("plugin listener finished");
            },
        }

        Ok(())
    }

    async fn handle_client_connection(&mut self, stream: UnixStream) {
        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let write_half = Arc::new(Mutex::new(write_half));

        // plugin -> daemon -> client
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<JSONRPCResponse>();
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let mut writer = write_half.lock().await;
                let serialized = message.to_json();
                if let Err(e) = serialized {
                    tracing::error!(
                        "failed to serialize daemon response: {:?} -> {}",
                        &message,
                        e
                    );
                    continue;
                }

                let serialized = serialized.unwrap();
                let json_str = format!("{}\n", serialized);
                if let Err(e) = writer.write_all(json_str.as_bytes()).await {
                    tracing::error!("failed to write message: {}", e);
                    break;
                }
            }
        });

        self.client_counter += 1;
        let client_id = self.client_counter;
        let mut connections = self.client_connections.lock().await;
        connections.insert(
            client_id,
            ClientConnection {
                client_id,
                writer: tx,
            },
        );
        drop(connections);

        // client -> daemon -> plugins
        let mut line = String::new();
        loop {
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    tracing::debug!("client disconnected");
                    break;
                }
                Ok(_) => {
                    if line.is_empty() {
                        continue;
                    }
                    tracing::debug!("received client message: {}", line);
                    let message = JSONRPCRequest::from_string(&line);
                    if let Err(e) = message {
                        tracing::error!("failed to parse client message: {} -> {}", &line, e);
                        continue;
                    }
                    let message = message.unwrap();
                    for plugin in self.plugin_connections.lock().await.values() {
                        if let Err(e) = plugin.writer.send(message.clone()) {
                            tracing::error!(
                                "failed to send message to plugin {}: {}",
                                plugin.plugin_id,
                                e
                            );
                        }
                    }
                    line.clear();
                }
                Err(e) => {
                    tracing::error!("failed to read from client: {}", e);
                    break;
                }
            }
        }

        tracing::debug!("client connection closed, client_id: {}", &client_id);
        let mut connections = self.client_connections.lock().await;
        connections.remove(&client_id);
        tracing::info!(
            "client removed, client_id: {}, remaining connections: {}",
            &client_id,
            connections.len()
        );

        tracing::info!(
            "client disconnected, remaining connections: {}",
            connections.len()
        );
        drop(connections);
    }

    pub async fn handle_plugin_connection(&mut self, stream: UnixStream) {
        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let write_half = Arc::new(Mutex::new(write_half));
        self.plugin_counter += 1;
        let plugin_id = self.plugin_counter;

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<JSONRPCRequest>();
        let connection = PluginConnection {
            plugin_id,
            writer: tx,
        };

        self.plugin_connections
            .lock()
            .await
            .insert(plugin_id, connection.clone());
        tracing::info!("plugin inserted, plugin_id: {}", &plugin_id);

        // client -> daemon -> plugin
        let writer_handle = tokio::spawn(async move {
            tracing::debug!("plugin writer task for {} started", &plugin_id);
            while let Some(message) = rx.recv().await {
                let mut writer = write_half.lock().await;
                let serialized = serde_json::to_string(&message);
                if let Err(e) = serialized {
                    tracing::error!("failed to serialize client message: {:?}, {}", &message, e);
                    continue;
                }
                let serialized = serialized.unwrap();
                let json_str = format!("{}\n", serialized);
                match writer.write_all(json_str.as_bytes()).await {
                    Ok(_) => tracing::debug!("sent message to plugin: {:?}", &message),
                    Err(e) => {
                        tracing::error!(
                            "failed to write client message to plugin: {:?} {}",
                            &message,
                            e
                        );
                        continue;
                    }
                }
            }
            tracing::debug!("plugin writer task for {} finished", &plugin_id);
        });

        // plugin -> daemon -> clients
        let reader_handle = tokio::spawn(async move {
            tracing::debug!("plugin reader task for {} started", &plugin_id);
            let mut line = String::new();
            loop {
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        break;
                    }
                    Ok(_) => {
                        if line.is_empty() {
                            continue;
                        }
                        tracing::debug!("received plugin message: {}", line);
                    }
                    Err(e) => {
                        tracing::error!("failed to read from plugin: {}", e);
                        break;
                    }
                }
                line.clear();
            }

            tracing::debug!("plugin reader task for {} finished", &plugin_id);
        });

        tracing::debug!("plugin connection established, plugin_id: {}", &plugin_id);
        tokio::select! {
            _ = writer_handle => {
                tracing::debug!("plugin writer task finished for plugin_id: {}", &plugin_id);
            },
            _ = reader_handle => {
                tracing::debug!("plugin reader task finished for plugin_id: {}", &plugin_id);
            },
        }

        self.plugin_connections
            .lock()
            .await
            .remove(&connection.plugin_id);

        tracing::info!("plugin disconnected");
    }
}

async fn safe_bind(path: &std::path::PathBuf) -> anyhow::Result<UnixListener> {
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
