use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use glimpse_sdk::{Action, Match, Message, Metadata, Method, MethodResult};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, stdin, stdout},
    sync::{Mutex, mpsc},
};

use crate::{
    dispatchers,
    plugins::{PluginResponse, discover_plugins, spawn_plugin},
};

struct ConnectedPlugin {
    metadata: Option<Metadata>,
    tx: mpsc::Sender<Message>,
}

struct MatchHolder {
    plugin_id: String,
    match_: Match,
}

pub struct Daemon {
    current_request: Arc<AtomicUsize>,
    current_matches: Arc<Mutex<Vec<MatchHolder>>>,
    stop_channel: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Default for Daemon {
    fn default() -> Self {
        Self::new()
    }
}

impl Daemon {
    pub fn new() -> Self {
        let (stop_channel, _) = tokio::sync::oneshot::channel();
        Daemon {
            current_request: Arc::new(AtomicUsize::new(0)),
            stop_channel: Some(stop_channel),
            current_matches: Arc::new(Mutex::new(vec![])),
        }
    }

    pub async fn stop(&mut self) {
        if let Some(stop_channel) = self.stop_channel.take() {
            let _ = stop_channel.send(());
        }
    }

    pub async fn run(&mut self) {
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
        let current_matches = self.current_matches.clone();
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
                                    MethodResult::Matches { items } => {
                                        let new_items = items
                                            .iter()
                                            .map(|m| MatchHolder {
                                                plugin_id: plugin_id.clone(),
                                                match_: m.clone(),
                                            })
                                            .collect::<Vec<_>>();
                                        current_matches.lock().await.extend(new_items);
                                        let _ = response_tx.send(message.clone()).await;
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
        let current_matches = self.current_matches.clone();
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
                    Message::Request {
                        id,
                        method,
                        ref plugin_id,
                    } => match method {
                        Method::Search(query) => {
                            current_request.store(id, Ordering::SeqCst);
                            current_matches.lock().await.clear();

                            for plugin in plugins_copy.lock().await.values() {
                                if plugin_id.is_some() {
                                    if plugin.metadata.is_none() {
                                        continue;
                                    }

                                    let connected_plugin_id = plugin.metadata.clone().unwrap().id;
                                    if plugin_id.clone().unwrap() != connected_plugin_id {
                                        continue;
                                    }
                                }

                                let tx = plugin.tx.clone();
                                let request = Message::Request {
                                    id,
                                    method: Method::Search(query.clone()),
                                    plugin_id: None,
                                };
                                tokio::spawn(async move {
                                    if let Err(e) = tx.send(request).await {
                                        tracing::error!("failed to send request to plugin: {}", e);
                                    }
                                });
                            }
                        }
                        Method::Activate(match_index, action_index) => {
                            let matches = current_matches.lock().await;
                            if match_index >= matches.len() {
                                tracing::warn!("invalid match index: {}", &match_index);
                                continue;
                            }

                            if action_index >= matches[match_index].match_.actions.len() {
                                tracing::warn!("invalid action index: {}", &action_index);
                                continue;
                            }

                            let action = &matches[match_index].match_.actions[action_index].action;
                            match action {
                                Action::Exec { command, args } => {
                                    dispatchers::shell_exec(&command, args).await
                                }
                                Action::Launch {
                                    app_id,
                                    args,
                                    new_instance,
                                } => dispatchers::launch_app(&app_id, &args, *new_instance).await,
                                Action::Clipboard { text } => {
                                    dispatchers::copy_to_clipboard(&text).await
                                }
                                Action::Open { uri } => dispatchers::open_url(&uri).await,
                                Action::Callback { key, params } => {
                                    let source_plugin_id = matches[match_index].plugin_id.clone();
                                    let plugin_tx = plugins_copy
                                        .lock()
                                        .await
                                        .get(&source_plugin_id)
                                        .map(|p| p.tx.clone());
                                    if let Some(tx) = plugin_tx {
                                        dispatchers::plugin_callback(tx, &key, &params).await;
                                    } else {
                                        tracing::warn!(
                                            "failed to find plugin for callback: {}",
                                            source_plugin_id
                                        );
                                    }
                                }
                            }
                        }
                        Method::Cancel => {
                            current_request.store(0, Ordering::SeqCst);
                            current_matches.lock().await.clear();
                            for plugin in plugins_copy.lock().await.values() {
                                let tx = plugin.tx.clone();
                                let request = Message::Request {
                                    id,
                                    method: Method::Cancel,
                                    plugin_id: None,
                                };
                                tokio::spawn(async move {
                                    if let Err(e) = tx.send(request).await {
                                        tracing::error!("failed to send cancel to plugin: {}", e);
                                    }
                                });
                            }
                        }
                        Method::Quit => {
                            tracing::info!("received quit command, shutting down");
                            for plugin in plugins_copy.lock().await.values() {
                                let tx = plugin.tx.clone();
                                let request = Message::Request {
                                    id,
                                    method: Method::Quit,
                                    plugin_id: None,
                                };
                                tokio::spawn(async move {
                                    if let Err(e) = tx.send(request).await {
                                        tracing::error!("failed to send cancel to plugin: {}", e);
                                    }
                                });
                            }
                            break;
                        }
                        Method::CallAction(key, params) => {
                            tracing::warn!("unexpected CallAction method from client: {} {:?}", key, params);
                        }
                    },
                    Message::Notification { method, .. } => match method {
                        _ => {}
                    },
                    Message::Response { .. } => {}
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
