use anyhow::Result;
use glimpse_core::ipc::{cli, sunset_socket_path};

pub use cli::{DispatchArgs, WatchArgs};

pub async fn watch(args: WatchArgs) -> Result<()> {
    cli::watch(args, sunset_socket_path()).await
}

pub async fn dispatch(args: DispatchArgs) -> Result<()> {
    cli::dispatch(args, sunset_socket_path()).await
}
