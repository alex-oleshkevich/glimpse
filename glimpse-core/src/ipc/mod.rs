pub mod cli;
pub mod client;
pub mod dispatcher;
pub mod protocol;
pub mod server;

pub use server::{
    IpcEmitter, IpcHandle, IpcServer, applets_socket_path, new_event_channel, resolve_socket_path,
    shell_socket_path, wallpaper_socket_path,
};

use std::sync::Arc;
use tokio::sync::broadcast;

/// Emit a named event with key-value fields to a broadcast channel.
/// Intended for use by daemon IPC modules.
pub fn emit(
    tx: &broadcast::Sender<Arc<protocol::IpcEvent>>,
    name: &str,
    fields: Vec<(&str, String)>,
) {
    let owned: Vec<(String, String)> = fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    let _ = tx.send(Arc::new(protocol::IpcEvent::new(name, owned)));
}
