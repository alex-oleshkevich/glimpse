use std::{os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc};

use tokio::net::UnixListener;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::services::framework::Services;

use super::{client::{CommandHandler, IpcClientHandler}, dispatcher, protocol::IpcEvent};

const BROADCAST_CAPACITY: usize = 256;

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

    /// A cloneable, `Send` emitter for code that produces events away from the
    /// `IpcHandle` owner (e.g. shell panel/applet components on the GTK thread).
    pub fn emitter(&self) -> IpcEmitter {
        IpcEmitter {
            event_tx: self.event_tx.clone(),
        }
    }
}

/// Detached event emitter handed to subsystems that don't own the `IpcHandle`.
#[derive(Clone)]
pub struct IpcEmitter {
    event_tx: broadcast::Sender<Arc<IpcEvent>>,
}

impl IpcEmitter {
    pub fn emit(&self, name: &str, fields: Vec<(&str, String)>) {
        let owned: Vec<(String, String)> =
            fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        let _ = self.event_tx.send(Arc::new(IpcEvent::new(name, owned)));
    }
}

pub struct IpcServer;

impl IpcServer {
    /// Launch the IPC server for the shell. Starts the full service dispatcher
    /// and binds to the shell socket path.
    pub fn launch<H>(services: &Services, command_handler: H) -> IpcHandle
    where
        H: CommandHandler + Clone + Send + 'static,
    {
        let event_tx = dispatcher::start(services);
        Self::launch_at(event_tx, shell_socket_path(), command_handler)
    }

    /// Launch an IPC server at an arbitrary socket path with a caller-supplied
    /// broadcast channel. Used by non-shell daemons that manage their own
    /// event channels and socket paths.
    pub fn launch_at<H>(
        event_tx: broadcast::Sender<Arc<IpcEvent>>,
        socket_path: PathBuf,
        command_handler: H,
    ) -> IpcHandle
    where
        H: CommandHandler + Clone + Send + 'static,
    {
        let cancel = CancellationToken::new();
        {
            let cancel_task = cancel.clone();
            let path_clone = socket_path.clone();
            let tx_clone = event_tx.clone();
            tokio::spawn(async move {
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
                        accept_loop(listener, path_clone, tx_clone, command_handler, cancel_task)
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
        IpcHandle { event_tx, cancel }
    }
}

async fn accept_loop<H>(
    listener: UnixListener,
    path: PathBuf,
    tx: broadcast::Sender<Arc<IpcEvent>>,
    command_handler: H,
    cancel: CancellationToken,
) where
    H: CommandHandler + Clone + Send + 'static,
{
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            result = listener.accept() => match result {
                Ok((stream, _addr)) => {
                    let rx = tx.subscribe();
                    let handler = command_handler.clone();
                    tokio::spawn(async move {
                        IpcClientHandler::new(stream, rx, handler).run().await;
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

/// The per-user runtime directory for Glimpse IPC sockets.
///
/// Requires `XDG_RUNTIME_DIR` (a private, 0700, user-owned tmpfs). There is
/// deliberately no `/tmp` fallback: `/tmp` is world-writable, so a predictable
/// `/tmp/glimpse/*.sock` path invites socket pre-creation / symlink hijack and
/// cross-user DoS. A session without `XDG_RUNTIME_DIR` is misconfigured and we
/// fail fast rather than bind an insecure socket.
fn runtime_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR").expect(
        "XDG_RUNTIME_DIR is not set; refusing to create an IPC socket under /tmp \
         (insecure). Run inside a proper user session.",
    );
    PathBuf::from(base).join("glimpse")
}

pub fn shell_socket_path() -> PathBuf   { runtime_dir().join("ipc.sock") }
pub fn idle_socket_path() -> PathBuf    { runtime_dir().join("idle.sock") }
pub fn sunset_socket_path() -> PathBuf  { runtime_dir().join("sunset.sock") }
pub fn wallpaper_socket_path() -> PathBuf { runtime_dir().join("wallpaper.sock") }

/// Kept for backwards compatibility — same as `shell_socket_path()`.
pub fn resolve_socket_path() -> PathBuf { shell_socket_path() }

/// Create a new broadcast channel suitable for use with `IpcServer::launch_at`.
pub fn new_event_channel() -> broadcast::Sender<Arc<IpcEvent>> {
    broadcast::channel(BROADCAST_CAPACITY).0
}
