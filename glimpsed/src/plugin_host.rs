use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicI16, Ordering},
    },
};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{
        UnixListener,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{Mutex, mpsc},
};

static PLUGIN_ID: AtomicI16 = AtomicI16::new(0);

struct ProcessPlugin {
    command: PathBuf,
}

pub struct PluginHost {
    input: mpsc::Receiver<String>,
    output: mpsc::Sender<String>,
    connections: Arc<Mutex<Vec<PluginConnHandle>>>,
}

impl PluginHost {
    pub fn new(input: mpsc::Receiver<String>, output: mpsc::Sender<String>) -> Self {
        PluginHost {
            input,
            output,
            connections: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let socket_path = dirs::runtime_dir()
            .expect("failed to get runtime directory")
            .join("glimpse-rpc.sock");
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }
        tracing::info!(
            "listening for plugin connections on {}",
            socket_path.display()
        );

        // launch plugin processes
        let plugins = self.discover_plugins().await?;
        for plugin in plugins {
            tracing::info!("starting plugin: {:?}", plugin.command);
            let socket_path = socket_path.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_plugin_process(&plugin, socket_path).await {
                    tracing::error!("plugin process crashed: {}", e);
                }
            });
        }

        // deliver messages to plugins
        let input = self.input;
        let connections_for_dispatch = Arc::clone(&self.connections);
        tokio::spawn(async move {
            let mut rx = input;
            while let Some(msg) = rx.recv().await {
                let mut connections = connections_for_dispatch.lock().await;
                for conn in connections.iter_mut() {
                    if let Err(e) = conn.write(&msg).await {
                        tracing::error!("failed to send message to plugin: {}", e);
                    }
                }
                tracing::info!("dispatched message to {} plugins", connections.len());
                drop(connections); // Explicitly drop the lock before returning
                tracing::info!("message sent: {}", msg);
            }
        });

        // listen for plugin connections
        let listener = UnixListener::bind(&socket_path)?;
        while let Ok((stream, _)) = listener.accept().await {
            tracing::info!(
                "accepted plugin connection from {:?}",
                stream.peer_addr().unwrap()
            );
            let connections = Arc::clone(&self.connections);
            let tx = self.output.clone();
            tokio::spawn(async move {
                let (reader, writer) = stream.into_split();
                let next_id = PLUGIN_ID.fetch_add(1, Ordering::SeqCst);
                let handle = PluginConnHandle {
                    id: next_id,
                    writer,
                };
                connections.lock().await.push(handle);
                if let Err(e) = handle_client(reader, tx).await {
                    tracing::error!("error handling plugin connection: {}", e);
                } else {
                    tracing::info!("plugin disconnected")
                }
                // Remove the connection from the list
                let mut connections = connections.lock().await;
                connections.retain(|c| c.id != next_id); // Retain only those not equal to the disconnected one
            });
        }
        Ok(())
    }

    async fn discover_plugins(&self) -> Result<Vec<ProcessPlugin>, Box<dyn std::error::Error>> {
        let mut plugins: Vec<ProcessPlugin> = Vec::new();
        let paths = extension_paths();
        tracing::info!("looking for plugins in: {:?}", paths);
        for path in paths {
            let extensions = load_extensions(&path);
            plugins.extend(extensions);
        }
        tracing::info!("loaded {} plugins", plugins.len());
        Ok(plugins)
    }
}

struct PluginConnHandle {
    id: i16,
    writer: OwnedWriteHalf,
}

impl PluginConnHandle {
    async fn write(&mut self, msg: &str) -> Result<(), std::io::Error> {
        self.writer.write_all(msg.as_bytes()).await
    }
}

async fn handle_client(
    reader: OwnedReadHalf,
    tx: mpsc::Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                if let Err(e) = tx.send(line.clone()).await {
                    tracing::error!("failed to send message: {}", e);
                    break;
                }
                tracing::info!("received: {}", line.trim());
                line.clear();
            }
            Err(e) => {
                eprintln!("Failed to read from socket: {}", e);
                break;
            }
        }
    }
    Ok(())
}

fn extension_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(data_dir) = dirs::data_dir() {
        let plugins_dir = data_dir.join("glimpse").join("plugins");
        if plugins_dir.exists() {
            paths.push(plugins_dir);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let local_path = cwd.join("plugins");
        paths.push(local_path);
    }

    paths
}

fn load_extensions(path: &PathBuf) -> Vec<ProcessPlugin> {
    let mut extensions = Vec::new();
    tracing::info!("looking for extensions in: {:?}", path);
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                let path_metadata = entry.path().metadata();
                if let Err(e) = path_metadata {
                    tracing::error!("failed to read metadata for {:?}: {}", entry.path(), e);
                    continue;
                }

                let permissions = path_metadata.unwrap().permissions();
                let mode = permissions.mode();
                if mode & 0o111 == 0 {
                    tracing::warn!("skipping non-executable file: {:?}", entry.path());
                    continue;
                }

                extensions.push(ProcessPlugin {
                    command: entry.path(),
                });
                tracing::info!("loaded extension: {:?}", entry.path());
            }
        }
    }
    extensions
}

async fn handle_plugin_process(
    plugin: &ProcessPlugin,
    socket_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut child = tokio::process::Command::new(&plugin.command);
    tracing::debug!("running plugin: {:?}", child);

    for _ in 0..5 {
        match child.arg(socket_path.clone()).kill_on_drop(true).spawn() {
            Ok(mut child) => {
                tracing::debug!("plugin process started: {:?}", child.id());
                let status = child.wait().await?;
                if status.success() {
                    tracing::info!("plugin process exited successfully");
                    return Ok(());
                }
                tracing::warn!("plugin process exited with non-zero status: {}", status);
            }
            Err(e) => {
                tracing::error!("failed to start plugin process: {}", e);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "plugin process failed to start after multiple attempts",
    )))
}
