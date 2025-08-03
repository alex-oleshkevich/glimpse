use anyhow;
use glimpse_sdk::{safe_bind, JSONRPCRequest, JSONRPCResponse};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::{Mutex, mpsc::UnboundedSender},
};

struct ClientConnection {
    client_id: usize,
    writer: UnboundedSender<JSONRPCResponse>,
}

impl ClientConnection {
    async fn send_message(&self, message: JSONRPCResponse) -> anyhow::Result<()> {
        self.writer.send(message).map_err(|e| {
            anyhow::anyhow!("failed to send message to client {}: {}", self.client_id, e)
        })
    }
}

#[derive(Clone)]
struct PluginConnection {
    plugin_id: usize,
    writer: UnboundedSender<JSONRPCRequest>,
}

impl PluginConnection {
    async fn send_message(&self, message: JSONRPCRequest) -> anyhow::Result<()> {
        self.writer.send(message).map_err(|e| {
            anyhow::anyhow!("failed to send message to plugin {}: {}", self.plugin_id, e)
        })
    }
}

#[derive(Clone)]
pub struct Daemon {
    client_counter: Arc<AtomicUsize>,
    plugin_counter: Arc<AtomicUsize>,
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
            client_counter: Arc::new(AtomicUsize::new(0)),
            plugin_counter: Arc::new(AtomicUsize::new(0)),
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

        let client_id = self.client_counter.fetch_add(1, Ordering::SeqCst);

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<JSONRPCResponse>();
        self.client_connections.lock().await.insert(
            client_id,
            ClientConnection {
                client_id,
                writer: tx,
            },
        );

        // plugin -> daemon -> client
        let reader_handle = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let mut writer = write_half.lock().await;
                let serialized = message.to_string();
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

        // client -> daemon -> plugins
        let daemon_clone = self.clone();
        let writer_handle = tokio::spawn(async move {
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
                        tracing::debug!("received client request: {}", line);

                        let message = JSONRPCRequest::from_string(&line);
                        if let Err(e) = message {
                            tracing::error!("failed to parse client request: {} -> {}", &line, e);
                            continue;
                        }

                        let message = message.unwrap();
                        if let Err(e) = daemon_clone.send_to_plugin(message).await {
                            tracing::error!("failed to forward request to plugin: {}", e);
                            continue;
                        }
                        line.clear();
                    }
                    Err(e) => {
                        tracing::error!("failed to read from client: {}", e);
                        break;
                    }
                }
            }
        });

        tokio::select! {
            _ = reader_handle => {
                tracing::debug!("client reader task finished for client_id: {}", &client_id);
            },
            _ = writer_handle => {
                tracing::debug!("client writer task finished for client_id: {}", &client_id);
            },
        }

        self.client_connections.lock().await.remove(&client_id);
        tracing::debug!("client connection closed, client_id: {}", &client_id);
    }

    async fn handle_plugin_connection(&mut self, stream: UnixStream) {
        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let write_half = Arc::new(Mutex::new(write_half));
        let plugin_id = self.plugin_counter.fetch_add(1, Ordering::SeqCst);

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<JSONRPCRequest>();
        let connection = PluginConnection {
            plugin_id,
            writer: tx,
        };

        self.plugin_connections
            .lock()
            .await
            .insert(plugin_id, connection.clone());

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
                if let Err(e) = writer.write_all(json_str.as_bytes()).await {
                    tracing::error!(
                        "failed to write client message to plugin: {:?} {}",
                        &message,
                        e
                    );
                    continue;
                }
            }
            tracing::debug!("plugin writer task for {} finished", &plugin_id);
        });

        // plugin -> daemon -> clients
        let daemon_clone = self.clone();
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

                        let response = JSONRPCResponse::from_string(&line);
                        if let Err(e) = response {
                            tracing::error!("failed to parse plugin response: {} -> {}", &line, e);
                            continue;
                        }
                        let response = response.unwrap().with_plugin_id(plugin_id);
                        tracing::debug!("received plugin response: {}", &line);
                        daemon_clone
                            .send_to_clients(response)
                            .await
                            .unwrap_or_else(|e| {
                                tracing::error!("failed to send response to clients: {}", e);
                            });
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

    async fn send_to_clients(&self, response: JSONRPCResponse) -> anyhow::Result<()> {
        for client in self.client_connections.lock().await.values() {
            if let Err(e) = client.send_message(response.clone()).await {
                tracing::error!(
                    "failed to forward response to client {}: {}",
                    client.client_id,
                    e
                );
            }
        }
        Ok(())
    }

    async fn send_to_plugin(&self, request: JSONRPCRequest) -> anyhow::Result<()> {
        for plugin in self.plugin_connections.lock().await.values() {
            if let Err(e) = plugin.send_message(request.clone()).await {
                tracing::error!(
                    "failed to forward request to plugin {}:  {}",
                    plugin.plugin_id,
                    e
                );
            }
        }
        Ok(())
    }
}
