use std::{env::args, path::PathBuf, process::Stdio};
use tokio::{io::AsyncBufReadExt, process::Command, sync::mpsc};

use crate::{
    app::AppMessage,
    extensions::{ExtensionError, Response},
};

#[derive(Debug)]
pub struct ProcessHandle {
    plugin_tx: mpsc::Sender<AppMessage>,
    handle: tokio::task::JoinHandle<()>,
}

#[derive(Debug)]
pub enum ProcessError {}

impl ProcessHandle {
    pub fn new(path: PathBuf, app_tx: mpsc::Sender<AppMessage>) -> Result<Self, ProcessError> {
        let (plugin_tx, plugin_rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { ProcessHandle::run(path, app_tx, plugin_rx).await });
        Ok(ProcessHandle { plugin_tx, handle })
    }

    pub async fn dispatch(&self, request: AppMessage) -> Result<(), ExtensionError> {
        match self.plugin_tx.send(request).await {
            Ok(_) => Ok(tracing::debug!("dispatched request to plugin")),
            Err(err) => Err(ExtensionError::DispatchError(format!(
                "failed to send request to plugin: {}",
                err
            ))),
        }
    }

    async fn run(
        path: PathBuf,
        app_tx: mpsc::Sender<AppMessage>,
        plugin_rx: mpsc::Receiver<AppMessage>,
    ) {
        let mut child = match Command::new(&path)
            .arg("--stdio")
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                tracing::error!("failed to start plugin process: {}", err);
                return;
            }
        };

        tracing::debug!("plugin process started: {:?}", path);

        // handle stdin
        if let Some(stdin) = child.stdin.take() {
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                let mut stdin = stdin;

                let mut plugin_rx = plugin_rx;
                while let Some(request) = plugin_rx.recv().await {
                    match request {
                        AppMessage::Request(req) => {
                            let serialized = req.to_string();
                            if serialized.is_err() {
                                tracing::error!(
                                    "failed to serialize request: {}",
                                    serialized.err().unwrap()
                                );
                                continue;
                            }
                            let serialized = serialized.unwrap();

                            tracing::debug!("plugin request: {}", serialized);
                            if let Err(err) = stdin.write_all(serialized.as_bytes()).await {
                                tracing::error!("failed to write to plugin stdin: {}", err);
                                break;
                            }
                            if let Err(err) = stdin.write_all(b"\n").await {
                                tracing::error!("failed to write newline to plugin stdin: {}", err);
                                break;
                            }
                            if let Err(err) = stdin.flush().await {
                                tracing::error!("failed to flush plugin stdin: {}", err);
                                break;
                            }
                        }
                        _ => {
                            tracing::warn!("received unexpected message type: {:?}", request);
                            continue;
                        }
                    }
                }
                drop(stdin);
            });
        }

        // handle stdout
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                let mut line_reader = tokio::io::BufReader::new(stdout).lines();
                while let Ok(Some(line)) = line_reader.next_line().await {
                    tracing::debug!("plugin response: {}", line);
                    match Response::from_json(&line) {
                        Ok(response) => {
                            tracing::debug!("plugin response type: {:?}", response);
                            if let Err(err) = app_tx.send(AppMessage::Response(response)).await {
                                tracing::error!("failed to send response to app: {}", err);
                            }
                        }
                        Err(err) => {
                            tracing::error!("plugin response error: {}", err);
                        }
                    }
                }
            });
        }

        match child.wait().await {
            Ok(status) => {
                if status.success() {
                    tracing::info!("plugin process exited successfully: {:?}", path);
                } else {
                    tracing::error!(
                        "plugin process exited with error: {:?}, status: {}",
                        path,
                        status
                    );
                }
            }
            Err(err) => {
                tracing::error!("failed to wait for plugin process: {}", err);
            }
        }
    }
}
