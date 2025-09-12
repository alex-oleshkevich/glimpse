use std::path::PathBuf;

use glimpse_sdk::{Message, Metadata};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, stdin, stdout},
    sync::mpsc,
    time,
};

struct ActivePlugin {
    path: PathBuf,
    metadata: Option<Metadata>,
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
}

pub struct Daemon {
    plugins: Vec<ActivePlugin>,
}

impl Daemon {
    pub fn new() -> Self {
        Daemon {
            plugins: Vec::new(),
        }
    }

    pub async fn stop(&self) {}

    pub async fn run(&self) {
        // 6. maintain current request state (for cancellations)
        let stdin = stdin();
        let mut stdout = stdout();
        let mut reader = BufReader::new(stdin);

        let (response_tx, mut response_rx) = mpsc::channel::<Message>(10);

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
                tracing::debug!("request: {:?}", &message);
                match message {
                    Message::Request { id, method, .. } => {
                        // forward to appropriate plugin
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
                tracing::debug!("response: {:?}", &message);
                stdout.write_all(response.as_bytes()).await.unwrap();
                stdout.write_all(b"\n").await.unwrap();
                stdout.flush().await.unwrap();
            }
        });

        spawn_plugins(&self.plugins, response_tx.clone());

        tokio::select! {
            _ = stdin_handle => {},
            _ = stdout_handle => {},
        }
    }

    pub fn discover_plugins(self) -> Result<Self, anyhow::Error> {
        Ok(self)
    }
}

async fn spawn_plugins(plugins: &Vec<ActivePlugin>, response_tx: mpsc::Sender<Message>) {
    for plugin in plugins {
        let command = plugin.path.clone();
        let response_tx = response_tx.clone();
        let mut plugin_rx = plugin.rx;
        tokio::spawn(async move {
            handle_plugin(command, plugin_rx, response_tx).await;
        });
    }
}

async fn handle_plugin(
    path: PathBuf,
    plugin_rx: mpsc::Receiver<Message>,
    response_tx: mpsc::Sender<Message>,
) {
    loop {
        let status = tokio::process::Command::new(&path).spawn();
        if let Err(e) = status {
            tracing::error!("failed to start plugin {:?}: {}", path, e);
            time::sleep(time::Duration::from_secs(5)).await;
            continue;
        }
        let mut process = status.unwrap();

        let stdout = process.stdout.take().unwrap();
        let stdin = process.stdin.take().unwrap();
        let mut reader = BufReader::new(stdout);
        let mut writer = stdin;

        let stdout_handle = tokio::spawn(async move {
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
                        tracing::warn!("failed to parse plugin JSON: {}", err);
                        continue;
                    }
                };
                tracing::debug!("plugin response: {:?}", &message);
                if let Err(e) = response_tx.send(message).await {
                    tracing::error!("failed to send plugin response: {}", e);
                    break;
                }
            }
        });

        let stdin_handle = tokio::spawn(async move {
            while let Some(message) = plugin_rx.recv().await {
                let request = serde_json::to_string(&message).unwrap();
                tracing::debug!("plugin request: {:?}", &message);
                if let Err(e) = writer.write_all(request.as_bytes()).await {
                    tracing::error!("failed to write to plugin stdin: {}", e);
                    break;
                }
                if let Err(e) = writer.write_all(b"\n").await {
                    tracing::error!("failed to write newline to plugin stdin: {}", e);
                    break;
                }
                if let Err(e) = writer.flush().await {
                    tracing::error!("failed to flush plugin stdin: {}", e);
                    break;
                }
            }
        });
        tokio::select! {
            _ = stdin_handle => {},
            _ = stdout_handle => {},
            status = process.wait() => {
                match status {
                    Ok(exit_status) => {
                        tracing::warn!("plugin {:?} exited with status: {}", path, exit_status);
                    }
                    Err(e) => {
                        tracing::error!("failed to wait for plugin {:?}: {}", path, e);
                    }
                }
            }
        }
    }
}
