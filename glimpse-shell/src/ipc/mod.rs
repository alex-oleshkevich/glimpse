pub mod cli;

pub use glimpse_core::ipc::{IpcHandle, IpcServer};
pub use glimpse_core::ipc::server::resolve_socket_path;

use std::pin::Pin;

use glimpse_core::ipc::client::CommandHandler;
use crate::services::{audio, framework::{ServiceHandle, Services}};

pub fn launch(services: &Services) -> IpcHandle {
    IpcServer::launch(services, ShellCommandHandler { audio: services.audio.clone() })
}

#[derive(Clone)]
pub(super) struct ShellCommandHandler {
    audio: ServiceHandle<audio::State, audio::Command>,
}

impl CommandHandler for ShellCommandHandler {
    fn execute<'a>(
        &'a self,
        name: &'a str,
        fields: &'a [(String, String)],
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
        let audio = self.audio.clone();
        let name = name.to_owned();
        let fields = fields.to_owned();
        Box::pin(async move { execute_shell_command(&name, &fields, &audio).await })
    }
}

async fn execute_shell_command(
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

#[cfg(test)]
mod tests;
