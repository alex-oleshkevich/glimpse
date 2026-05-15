pub mod cli;

pub use glimpse_core::ipc::{IpcHandle, IpcServer};
pub use glimpse_core::ipc::client::NoopCommandHandler;
pub use glimpse_core::ipc::server::resolve_socket_path;

use crate::services::framework::Services;

pub fn launch(services: &Services) -> IpcHandle {
    IpcServer::launch(services, NoopCommandHandler)
}

#[cfg(test)]
mod tests;
