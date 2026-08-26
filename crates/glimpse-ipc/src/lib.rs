mod client;
mod codec;
mod frame;
mod outbox;
pub mod pattern;
mod server;

pub use client::{Client, ConnectError, ConnectionState, Subscription};
pub use frame::{Body, CallError, ErrorCode, Event, Frame, Status};
pub use server::{ClientId, Handler, Publisher, Server, ServerError, Subscribed};

use std::path::{Path, PathBuf};

pub const PROTOCOL_VERSION: u32 = 1;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SOCKET_RELATIVE_PATH: &str = "glimpse/glimpsed.sock";

#[derive(Debug, thiserror::Error)]
#[error("XDG_RUNTIME_DIR is not set; a graphical session is expected to provide it")]
pub struct NoRuntimeDir;

pub fn socket_path(explicit: Option<&Path>) -> Result<PathBuf, NoRuntimeDir> {
    resolve(explicit, dirs::runtime_dir())
}

fn resolve(explicit: Option<&Path>, runtime_dir: Option<PathBuf>) -> Result<PathBuf, NoRuntimeDir> {
    match explicit {
        // Verbatim: `--socket` is a decision, so a typo has to surface as a failed connection
        // naming that path, never as a silent fall back to the default.
        Some(path) => Ok(path.to_owned()),
        None => runtime_dir
            .map(|dir| dir.join(SOCKET_RELATIVE_PATH))
            .ok_or(NoRuntimeDir),
    }
}
