use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use glimpse_sdk::{Message, Metadata, MethodResult};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, stdin, stdout},
    sync::{Mutex, mpsc},
};

use crate::plugins::{PluginResponse, discover_plugins, spawn_plugin};

struct ConnectedPlugin {
    metadata: Option<Metadata>,
    tx: mpsc::Sender<Message>,
}

pub struct Daemon {
    current_request: Arc<AtomicUsize>,
    stop_channel: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Daemon {
    pub fn new() -> Self {
        let (stop_channel, _) = tokio::sync::oneshot::channel();
        Daemon {
            current_request: Arc::new(AtomicUsize::new(0)),
            stop_channel: Some(stop_channel),
        }
    }

    pub async fn stop(&mut self) {
        if let Some(stop_channel) = self.stop_channel.take() {
            let _ = stop_channel.send(());
        }
    }

    pub async fn run(&mut self) {
        // 6. maintain current request state (for cancellations)
        let stdin = stdin();
        let mut stdout = stdout();
        let mut reader = BufReader::new(stdin);
        let current_request = Arc::clone(&self.current_request);

        let (response_tx, mut response_rx) = mpsc::channel::<Message>(10);
        let (plugin_tx, mut plugin_rx) = mpsc::channel::<PluginResponse>(10);

        let plugin_paths = discover_plugins();
        tracing::info!("discovered plugins: {:?}", &plugin_paths);

        let mut handles = vec![];
        let plugins: HashMap<String, ConnectedPlugin> = plugin_paths
            .into_iter()
            .map(|path| {
                tracing::debug!("starting plugin {:?}", &path);
                let (tx, rx) = mpsc::channel::<Message>(10);
                let plugin_tx = plugin_tx.clone();
                let path_copy = path.clone();
                let handle = tokio::spawn(async move {
                    spawn_plugin(path_copy, plugin_tx, rx).await;
                });
                handles.push(handle);
                let plugin_name = path.to_string();
                (plugin_name, ConnectedPlugin { metadata: None, tx })
            })
            .collect();

        let response_tx = response_tx.clone();
        let current_request_clone = Arc::clone(&current_request);

        let plugins_arc = Arc::new(Mutex::new(plugins));
        let plugins_copy = plugins_arc.clone();
        let plugin_handle = tokio::spawn(async move {
            while let Some(ref plugin_message) = plugin_rx.recv().await {
                match plugin_message {
                    PluginResponse::Response(plugin_id, message) => {
                        match message {
                            Message::Response { id, result, .. } => {
                                if *id != current_request_clone.load(Ordering::SeqCst) {
                                    continue;
                                }

                                if result.is_none() {
                                    let _ = response_tx.send(message.clone()).await;
                                    continue;
                                }

                                let result = result.as_ref().unwrap();
                                match result {
                                    MethodResult::Authenticate(metadata) => {
                                        plugins_copy.lock().await.get_mut(plugin_id).map(
                                            |plugin| {
                                                plugin.metadata.replace(metadata.clone());
                                            },
                                        );
                                        tracing::info!(
                                            "authenticated plugin {} v{}",
                                            metadata.name,
                                            metadata.version
                                        );
                                    }
                                    _ => {
                                        let _ = response_tx.send(message.clone()).await;
                                    }
                                }
                            }
                            _ => {
                                let _ = response_tx.send(message.clone()).await;
                            }
                        };
                    }
                }
            }
        });

        let plugins_copy = plugins_arc.clone();
        let stdin_handle = tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                let bytes_read = reader.read_line(&mut line).await.unwrap();
                if bytes_read == 0 {
                    break;
                }

                let message: Message = match serde_json::from_str(&line) {
                    Ok(msg) => msg,
                    Err(err) => {
                        tracing::warn!("failed to parse JSON: {}", err);
                        continue;
                    }
                };
                tracing::debug!("client request -> plugins: {:?}", &message);

                match message {
                    Message::Request { id, method, .. } => {
                        current_request.store(id, Ordering::SeqCst);

                        for plugin in plugins_copy.lock().await.values() {
                            let tx = plugin.tx.clone();
                            let method_clone = method.clone();
                            let request = Message::Request {
                                id,
                                method: method_clone,
                                target: None,
                                context: None,
                            };
                            tokio::spawn(async move {
                                if let Err(e) = tx.send(request).await {
                                    tracing::error!("failed to send request to plugin: {}", e);
                                }
                            });
                        }
                    }
                    Message::Notification { method } => match method {
                        _ => {}
                    },
                    _ => {}
                }
            }
        });

        let stdout_handle = tokio::spawn(async move {
            while let Some(message) = response_rx.recv().await {
                let response = serde_json::to_string(&message).unwrap();
                tracing::debug!("plugin response -> client: {:?}", &message);
                stdout.write_all(response.as_bytes()).await.unwrap();
                stdout.write_all(b"\n").await.unwrap();
                stdout.flush().await.unwrap();
            }
        });

        tokio::select! {
            _ = stdin_handle => {},
            _ = stdout_handle => {},
            _ = plugin_handle => {},
        }

        tracing::debug!("shutting down, waiting for plugins to exit");
        for handle in handles {
            let _ = handle.await;
        }

        tracing::debug!("all plugins exited, daemon shutting down");
    }
}
