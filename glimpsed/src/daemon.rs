use std::path::PathBuf;

use glimpse_sdk::{Message, Metadata};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, stdin, stdout},
    sync::mpsc,
};

use crate::plugins::{discover_plugins, spawn_plugin};

struct ActivePlugin {
    path: PathBuf,
    metadata: Option<Metadata>,
    tx: mpsc::Sender<Message>,
}

pub struct Daemon {}

impl Daemon {
    pub fn new() -> Self {
        Daemon {}
    }

    pub async fn stop(&self) {}

    pub async fn run(&self) {
        // 6. maintain current request state (for cancellations)
        let stdin = stdin();
        let mut stdout = stdout();
        let mut reader = BufReader::new(stdin);
        let (response_tx, mut response_rx) = mpsc::channel::<Message>(10);

        let plugin_paths = discover_plugins();
        tracing::info!("discovered plugins: {:?}", &plugin_paths);

        let mut handles = vec![];
        let plugins: Vec<ActivePlugin> = plugin_paths
            .into_iter()
            .map(|path| {
                tracing::debug!("starting plugin {:?}", &path);
                let (tx, rx) = mpsc::channel::<Message>(10);
                let response_tx = response_tx.clone();
                let path_copy = path.clone();
                let handle = tokio::spawn(async move {
                    spawn_plugin(path_copy, response_tx, rx).await;
                });
                handles.push(handle);
                ActivePlugin {
                    path: path.into(),
                    metadata: None,
                    tx,
                }
            })
            .collect();

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
                        for plugin in &plugins {
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
        }

        tracing::debug!("shutting down, waiting for plugins to exit");
        for handle in handles {
            let _ = handle.await;
        }

        tracing::debug!("all plugins exited, daemon shutting down");
    }
}
