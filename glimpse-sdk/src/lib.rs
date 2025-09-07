pub mod plugin;
pub mod protocol;

use std::{error::Error, fmt::Display, sync::Arc};

use tokio_util::sync::CancellationToken;

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, stdin, stdout},
    task::JoinHandle,
};

pub use plugin::*;
pub use protocol::*;

#[derive(Debug)]
pub enum PluginError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Cancelled(String),
    Other(String),
}

impl Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginError::Io(err) => write!(f, "IO error: {}", err),
            PluginError::Json(err) => write!(f, "JSON error: {}", err),
            PluginError::Other(msg) => write!(f, "error: {}", msg),
            PluginError::Cancelled(msg) => write!(f, "cancelled: {}", msg),
        }
    }
}
impl Error for PluginError {}

pub fn setup_logging(log_level: tracing::Level) {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_writer(std::io::stderr)
        .with_target(false)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}

pub async fn run_plugin<P: Plugin>(plugin: P) -> Result<(), PluginError> {
    let stdin = stdin();
    let mut stdout = stdout();
    let mut reader = BufReader::new(stdin);

    let (response_tx, mut response_rx) = tokio::sync::mpsc::channel::<Message>(10);

    // task cancellation
    let mut current_cancel_token: Option<CancellationToken> = None;
    let mut current_task: Option<JoinHandle<()>> = None;

    let self_ref = Arc::new(plugin);
    let response_tx_clone = response_tx.clone();

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

            tracing::debug!("received message: {:?}", &message);
            match message {
                Message::Request { id, method, .. } => {
                    if let Some(cancel_token) = current_cancel_token.take() {
                        tracing::debug!("cancelling previous request");
                        cancel_token.cancel();
                    }

                    if let Some(task) = current_task.take() {
                        task.abort();
                    }

                    // new cancellation token
                    let cancel_token = CancellationToken::new();
                    current_cancel_token = Some(cancel_token.clone());

                    let plugin_clone = self_ref.clone();
                    let response_tx = response_tx_clone.clone();

                    let task = tokio::spawn(async move {
                        let result = tokio::select! {
                            result = plugin_clone.handle(method) => result,
                            _ = cancel_token.cancelled() => {
                                tracing::debug!("request {} was cancelled", id);
                                Err(PluginError::Cancelled("request cancelled".into()))
                            },
                        };

                        let response = match result {
                            Ok(method_result) => Message::Response {
                                id,
                                error: None,
                                source: None,
                                result: Some(method_result),
                            },
                            Err(err) => Message::Response {
                                id,
                                error: Some(err.to_string()),
                                source: None,
                                result: None,
                            },
                        };

                        if let Err(err) = response_tx.send(response).await {
                            tracing::warn!("error sending response: {}", err);
                        }
                    });
                    current_task = Some(task);
                }
                Message::Notification { method } => match method {
                    Method::Cancel => {
                        if let Some(cancel_token) = current_cancel_token.take() {
                            tracing::debug!("manual cancel requested");
                            cancel_token.cancel();
                        }
                    }
                    Method::Quit => {
                        tracing::debug!("manual cancel requested");
                        break;
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    });

    let stdout_handle = tokio::spawn(async move {
        while let Some(message) = response_rx.recv().await {
            let response = serde_json::to_string(&message).unwrap();
            stdout.write_all(response.as_bytes()).await.unwrap();
            stdout.write_all(b"\n").await.unwrap();
            stdout.flush().await.unwrap();
        }
    });

    tokio::select! {
        _ = stdin_handle => {
            tracing::debug!("stdin closed, exiting");
        },
        _ = stdout_handle => {
            tracing::debug!("stdout write completed, exiting");
        },
    }

    Ok(())
}
