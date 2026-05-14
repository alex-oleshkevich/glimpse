use std::{collections::HashSet, sync::Arc};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::broadcast,
};

use crate::services::{audio, framework::ServiceHandle};

use super::protocol::{ClientMsg, IpcEvent, ack_line, hello_line, matches_pattern, parse_client_line};

const MAX_IPC_LINE: usize = 64 * 1024;

pub struct IpcClientHandler {
    stream: UnixStream,
    events: broadcast::Receiver<Arc<IpcEvent>>,
    audio: ServiceHandle<audio::State, audio::Command>,
}

impl IpcClientHandler {
    pub fn with_audio(
        stream: UnixStream,
        events: broadcast::Receiver<Arc<IpcEvent>>,
        audio: ServiceHandle<audio::State, audio::Command>,
    ) -> Self {
        Self { stream, events, audio }
    }


    pub async fn run(mut self) {
        let (reader, mut writer) = self.stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        let hello = format!("{}\n", hello_line());
        if writer.write_all(hello.as_bytes()).await.is_err() {
            return;
        }

        let mut subscriptions: HashSet<String> = HashSet::new();

        loop {
            tokio::select! {
                line = lines.next_line() => {
                    match line {
                        Ok(Some(line)) if !line.trim().is_empty() => {
                            if line.len() > MAX_IPC_LINE {
                                tracing::debug!(len = line.len(), "IPC client sent oversize line; disconnecting");
                                break;
                            }
                            match parse_client_line(line.trim()) {
                                Ok(ClientMsg::Subscribe(patterns)) => {
                                    subscriptions.extend(patterns);
                                }
                                Ok(ClientMsg::Unsubscribe(patterns)) => {
                                    for p in patterns {
                                        subscriptions.remove(&p);
                                    }
                                }
                                Ok(ClientMsg::Command { name, fields }) => {
                                    let result = execute_command(&name, &fields, &self.audio).await;
                                    let response = match result {
                                        Ok(()) => ack_line(true, None),
                                        Err(e) => ack_line(false, Some(&e)),
                                    };
                                    let _ = writer.write_all(format!("{response}\n").as_bytes()).await;
                                }
                                Err(e) => {
                                    tracing::debug!(error = %e, "IPC client sent unparseable line");
                                }
                            }
                        }
                        Ok(Some(_)) => {}
                        Ok(None) | Err(_) => break,
                    }
                }
                result = self.events.recv() => {
                    match result {
                        Ok(event) => {
                            if subscriptions.iter().any(|p| matches_pattern(p, &event.name)) {
                                let line = format!("{}\n", event.encode());
                                if writer.write_all(line.as_bytes()).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::debug!(dropped = n, "IPC client lagged; events dropped");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    }
}

async fn execute_command(
    name: &str,
    fields: &[(String, String)],
    audio: &ServiceHandle<audio::State, audio::Command>,
) -> Result<(), String> {
    let get = |key: &str| fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());

    match name {
        "open_uri" => {
            let uri = get("uri").ok_or("missing uri")?;
            let allowed = uri.starts_with("http://")
                || uri.starts_with("https://")
                || uri.starts_with("mailto:");
            if !allowed {
                return Err("open_uri only allows http, https, and mailto URIs".into());
            }
            let mut child = tokio::process::Command::new("xdg-open")
                .arg(uri)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(false)
                .spawn()
                .map_err(|e| format!("xdg-open failed: {e}"))?;
            tokio::spawn(async move { let _ = child.wait().await; });
            Ok(())
        }
        "set_volume" => {
            let level: u32 = get("level")
                .ok_or("missing level")?
                .parse()
                .map_err(|_| "level must be an integer 0–100")?;
            if level > 100 {
                return Err("level must be 0–100".into());
            }
            audio.try_send_command(
                "audio",
                audio::Command::SetOutputVolume(level),
                "failed to set volume",
            );
            Ok(())
        }
        "toggle_mute" => {
            audio.try_send_command(
                "audio",
                audio::Command::ToggleOutputMute,
                "failed to toggle mute",
            );
            Ok(())
        }
        _ => Err(format!("unknown command: {name}")),
    }
}
