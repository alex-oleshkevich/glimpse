pub mod cli;
mod handler;

pub use glimpse_core::ipc::server::{applets_socket_path, resolve_socket_path};
pub use glimpse_core::ipc::{IpcEmitter, IpcHandle, IpcServer};

use crate::services::framework::Services;
use handler::ShellCommandHandler;

pub fn launch(services: &Services, app_sender: relm4::Sender<crate::app::Input>) -> IpcHandle {
    IpcServer::launch(
        services,
        ShellCommandHandler {
            services: services.clone(),
            app_sender,
        },
    )
}

#[cfg(test)]
mod tests;
