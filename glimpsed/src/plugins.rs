use std::env;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use glimpse_sdk::Message;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, stderr as sys_stderr};
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::time;

pub fn discover_plugins() -> Vec<String> {
    let directories = vec![
        env::var("GLIMPSED_PLUGIN_DIR").unwrap_or_default(),
        dirs::data_dir()
            .map(|d| {
                d.join("glimpsed")
                    .join("plugins")
                    .to_string_lossy()
                    .to_string()
            })
            .unwrap_or_default(),
        "/usr/lib/glimpsed/plugins".to_owned(),
        "/usr/local/lib/glimpsed/plugins".to_owned(),
    ];

    let mut plugins = Vec::new();
    for dir in directories {
        if !std::path::Path::new(&dir).exists() {
            continue;
        }

        if dir.is_empty() {
            continue;
        }

        let entries = std::fs::read_dir(&dir);
        if let Err(err) = entries {
            tracing::warn!("failed to read plugin directory {}: {}", dir, err);
            continue;
        }
        let entries = entries.unwrap();
        for entry in entries.into_iter() {
            if let Err(err) = entry {
                tracing::warn!("failed to read plugin entry: {}", err);
                continue;
            }
            let entry = entry.unwrap();

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            #[cfg(unix)]
            {
                let metadata = match path.metadata() {
                    Ok(metadata) => metadata,
                    Err(err) => {
                        tracing::warn!("failed to read metadata for {}: {}", path.display(), err);
                        continue;
                    }
                };
                let permissions = metadata.permissions();
                if permissions.mode() & 0o111 == 0 {
                    continue;
                }
            }

            #[cfg(windows)]
            {
                // On Windows, check if it's a .exe or .dll file
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if ext != "exe" && ext != "dll" {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            let path_str = path.to_string_lossy().to_string();
            plugins.push(path_str);
        }
    }

    plugins
}

pub async fn spawn_plugin(
    path: String,
    response_tx: mpsc::Sender<Message>,
    plugin_rx: mpsc::Receiver<Message>,
) {
    let plugin_rx = Arc::new(Mutex::new(plugin_rx));

    loop {
        let status = tokio::process::Command::new(&path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();
        if let Err(e) = status {
            tracing::error!("failed to start plugin {:?}: {}", path, e);
            time::sleep(time::Duration::from_secs(5)).await;
            continue;
        }
        tracing::info!("started plugin {:?}", path);

        let mut process = status.unwrap();

        let stdin = process.stdin.take();
        if stdin.is_none() {
            tracing::error!("plugin {:?} has no stdin", path);
            return;
        }
        let stdin = stdin.unwrap();

        let stdout = process.stdout.take();
        if stdout.is_none() {
            tracing::error!("plugin {:?} has no stdout", path);
            return;
        }
        let stdout = stdout.unwrap();

        let stderr = process.stderr.take();
        if stderr.is_none() {
            tracing::error!("plugin {:?} has no stderr", path);
            return;
        }
        let stderr = stderr.unwrap();

        let mut reader = BufReader::new(stdout);
        let mut writer = stdin;

        let response_tx = response_tx.clone();

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

        let stderr_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                let bytes_read = reader.read_line(&mut line).await.unwrap();
                if bytes_read == 0 {
                    break;
                }

                let _ = sys_stderr().write_all(line.as_bytes()).await;
                let _ = sys_stderr().flush().await;
            }
        });

        let plugin_rx = plugin_rx.clone();
        let stdin_handle = tokio::spawn(async move {
            let mut plugin_rx = plugin_rx.lock().await;

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
            _ = stderr_handle => {},
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
