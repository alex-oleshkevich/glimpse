pub mod cli;
mod handler;

pub use glimpse_core::ipc::server::resolve_socket_path;
pub use glimpse_core::ipc::{IpcEmitter, IpcHandle, IpcServer};

use crate::services::framework::Services;
use handler::ShellCommandHandler;

pub fn launch(services: &Services) -> IpcHandle {
    IpcServer::launch(
        services,
        ShellCommandHandler {
            services: services.clone(),
        },
    )
}

#[cfg(test)]
mod tests;
