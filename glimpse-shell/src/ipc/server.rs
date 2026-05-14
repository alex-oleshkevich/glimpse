use std::{os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc};

use tokio::net::UnixListener;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::services::framework::Services;

use super::{client::IpcClientHandler, dispatcher, protocol::IpcEvent};

#[must_use]
pub struct IpcHandle {
    event_tx: broadcast::Sender<Arc<IpcEvent>>,
    cancel: CancellationToken,
}

impl Drop for IpcHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl IpcHandle {
    pub fn emit(&self, name: &str, fields: Vec<(&str, String)>) {
        let owned: Vec<(String, String)> = fields
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        let _ = self.event_tx.send(Arc::new(IpcEvent::new(name, owned)));
    }
}

pub struct IpcServer;

impl IpcServer {
    pub fn launch(services: &Services) -> IpcHandle {
        let socket_path = resolve_socket_path();
        let event_tx = dispatcher::start(services);
        let cancel = CancellationToken::new();

        {
            let cancel_task = cancel.clone();
            let services_clone = services.clone();
            let path_clone = socket_path.clone();
            let tx_clone = event_tx.clone();
            relm4::spawn(async move {
                if let Some(parent) = path_clone.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::remove_file(&path_clone);
                match UnixListener::bind(&path_clone) {
                    Ok(listener) => {
                        let _ = std::fs::set_permissions(
                            &path_clone,
                            std::fs::Permissions::from_mode(0o600),
                        );
                        tracing::info!(path = %path_clone.display(), "IPC socket ready");
                        accept_loop(listener, path_clone, tx_clone, services_clone, cancel_task)
                            .await;
                    }
                    Err(e) => {
                        tracing::warn!(
                            ?e,
                            path = %path_clone.display(),
                            "IPC socket bind failed; IPC disabled"
                        );
                    }
                }
            });
        }

        IpcHandle {
            event_tx,
            cancel,
        }
    }
}

async fn accept_loop(
    listener: UnixListener,
    path: PathBuf,
    tx: broadcast::Sender<Arc<IpcEvent>>,
    services: Services,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            result = listener.accept() => match result {
                Ok((stream, _addr)) => {
                    let rx = tx.subscribe();
                    let audio = services.audio.clone();
                    tokio::spawn(async move {
                        IpcClientHandler::with_audio(stream, rx, audio).run().await;
                    });
                }
                Err(e) => {
                    tracing::warn!(?e, "IPC accept error");
                }
            }
        }
    }
    let _ = std::fs::remove_file(&path);
}

pub fn resolve_socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(runtime_dir).join("glimpse").join("ipc.sock")
}
