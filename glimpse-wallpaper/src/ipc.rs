use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use glimpse_core::{
    FitMode, ResolvedBackdropSpec, ResolvedWallpaperSpec,
    ipc::{
        self, IpcHandle, IpcServer, client::CommandHandler, protocol::IpcEvent,
        wallpaper_socket_path,
    },
    services::theme::EffectiveThemeMode,
};
use tokio::sync::{broadcast, mpsc};

use crate::app::{ThemeModeRequest, WallpaperCommand};

pub fn start() -> (
    IpcHandle,
    broadcast::Sender<Arc<IpcEvent>>,
    mpsc::Receiver<WallpaperCommand>,
) {
    let tx = ipc::new_event_channel();
    let (cmd_tx, cmd_rx) = mpsc::channel::<WallpaperCommand>(16);
    let handle = IpcServer::launch_at(
        tx.clone(),
        wallpaper_socket_path(),
        WallpaperCommandHandler { cmd_tx },
    );
    (handle, tx, cmd_rx)
}

#[derive(Clone)]
struct WallpaperCommandHandler {
    cmd_tx: mpsc::Sender<WallpaperCommand>,
}

impl WallpaperCommandHandler {
    /// Await on the bounded channel so legitimate command bursts (multi-monitor
    /// reconcile, rapid theme toggles) queue instead of being dropped. Only a
    /// closed channel (daemon shutting down) yields an error.
    async fn send(&self, command: WallpaperCommand) -> Result<Vec<(String, String)>, String> {
        self.cmd_tx
            .send(command)
            .await
            .map_err(|_| "wallpaper daemon unavailable".to_owned())?;
        Ok(vec![])
    }
}

fn require<'a>(fields: &'a [(String, String)], key: &str) -> Result<&'a str, String> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| format!("missing {key}"))
}

fn parse_fit(value: &str) -> Result<FitMode, String> {
    match value {
        "cover" => Ok(FitMode::Cover),
        "contain" => Ok(FitMode::Contain),
        "fill" => Ok(FitMode::Fill),
        other => Err(format!(
            "mode must be cover, contain, or fill, got '{other}'"
        )),
    }
}

/// Reject anything that is not an absolute path to an existing regular file —
/// a bad path otherwise silently produces a blank wallpaper.
async fn validate_file(path: &str) -> Result<(), String> {
    let candidate = PathBuf::from(path);
    if !candidate.is_absolute() {
        return Err(format!("path must be absolute: {path}"));
    }
    let probe = candidate.clone();
    let is_file = tokio::task::spawn_blocking(move || {
        std::fs::metadata(&probe)
            .map(|m| m.is_file())
            .unwrap_or(false)
    })
    .await
    .map_err(|e| format!("path check failed: {e}"))?;
    if !is_file {
        return Err(format!("not an existing file: {path}"));
    }
    Ok(())
}

impl CommandHandler for WallpaperCommandHandler {
    fn execute<'a>(
        &'a self,
        name: &'a str,
        fields: &'a [(String, String)],
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<(String, String)>, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let get = |key: &str| {
                fields
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.as_str())
            };

            match name {
                "reload_config" => self.send(WallpaperCommand::ReloadConfig).await,

                "set_image" => {
                    let path = require(fields, "path")?;
                    validate_file(path).await?;
                    self.send(WallpaperCommand::SetImage(PathBuf::from(path)))
                        .await
                }

                "set_color" => {
                    let color = require(fields, "color")?.trim();
                    if color.is_empty() {
                        return Err("color must not be empty".to_owned());
                    }
                    color
                        .parse::<css_color::Srgb>()
                        .map_err(|_| format!("invalid color: {color}"))?;
                    self.send(WallpaperCommand::SetColor(color.to_owned())).await
                }

                "set_fit" => {
                    let fit = parse_fit(require(fields, "mode")?)?;
                    self.send(WallpaperCommand::SetFit(fit)).await
                }

                "set_backdrop" => {
                    let enabled = match require(fields, "enabled")? {
                        "true" => true,
                        "false" => false,
                        other => {
                            return Err(format!("enabled must be true or false, got '{other}'"));
                        }
                    };
                    let path = match get("path") {
                        Some(p) => {
                            validate_file(p).await?;
                            Some(PathBuf::from(p))
                        }
                        None => None,
                    };
                    let blur = match get("blur") {
                        Some(b) => Some(
                            b.parse::<u32>()
                                .map_err(|_| "blur must be a non-negative integer".to_owned())?,
                        ),
                        None => None,
                    };
                    self.send(WallpaperCommand::SetBackdrop {
                        enabled,
                        path,
                        blur,
                    })
                    .await
                }

                "set_theme_mode" => {
                    let request = match require(fields, "mode")? {
                        "light" => ThemeModeRequest::Light,
                        "dark" => ThemeModeRequest::Dark,
                        "auto" => ThemeModeRequest::Auto,
                        other => {
                            return Err(format!(
                                "mode must be light, dark, or auto, got '{other}'"
                            ));
                        }
                    };
                    self.send(WallpaperCommand::SetThemeMode(request)).await
                }

                _ => Err(format!("unknown command: {name}")),
            }
        })
    }
}

fn fit_name(fit: FitMode) -> &'static str {
    match fit {
        FitMode::Cover => "cover",
        FitMode::Contain => "contain",
        FitMode::Fill => "fill",
    }
}

pub fn emit_spec_changed(tx: &broadcast::Sender<Arc<IpcEvent>>, spec: &ResolvedWallpaperSpec) {
    let mut fields = vec![("color", spec.color.clone())];
    match &spec.image {
        Some(image) => {
            fields.push(("mode", "image".to_owned()));
            fields.push(("path", image.path.display().to_string()));
            fields.push(("fit", fit_name(image.fit).to_owned()));
        }
        None => fields.push(("mode", "color".to_owned())),
    }
    match &spec.backdrop {
        ResolvedBackdropSpec::Disabled => fields.push(("backdrop", "false".to_owned())),
        ResolvedBackdropSpec::Enabled { path, blur_radius } => {
            fields.push(("backdrop", "true".to_owned()));
            fields.push(("backdrop_blur", blur_radius.to_string()));
            if let Some(p) = path {
                fields.push(("backdrop_path", p.display().to_string()));
            }
        }
    }
    ipc::emit(tx, "wallpaper.spec_changed", fields);
}

pub fn emit_theme_changed(tx: &broadcast::Sender<Arc<IpcEvent>>, mode: EffectiveThemeMode) {
    let name = match mode {
        EffectiveThemeMode::Light => "light",
        EffectiveThemeMode::Dark => "dark",
    };
    ipc::emit(
        tx,
        "wallpaper.theme_changed",
        vec![("mode", name.to_owned())],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_queues_bursts_without_dropping() {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<WallpaperCommand>(16);
        let handler = WallpaperCommandHandler { cmd_tx };
        // Drain concurrently so the awaiting sender makes progress past the buffer.
        let consumer = tokio::spawn(async move {
            let mut count = 0usize;
            while cmd_rx.recv().await.is_some() {
                count += 1;
                if count == 50 {
                    break;
                }
            }
            count
        });
        for _ in 0..50 {
            handler
                .send(WallpaperCommand::ReloadConfig)
                .await
                .expect("command should queue, not be dropped");
        }
        assert_eq!(consumer.await.unwrap(), 50);
    }

    #[tokio::test]
    async fn send_errors_when_daemon_channel_closed() {
        let (cmd_tx, cmd_rx) = mpsc::channel::<WallpaperCommand>(1);
        drop(cmd_rx);
        let handler = WallpaperCommandHandler { cmd_tx };
        assert!(handler.send(WallpaperCommand::ReloadConfig).await.is_err());
    }
}
