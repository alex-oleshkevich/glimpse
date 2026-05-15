use anyhow::Result;
use glimpse_core::ipc::{cli, wallpaper_socket_path};

pub use cli::{DispatchArgs, WatchArgs};

pub async fn watch(args: WatchArgs) -> Result<()> {
    cli::watch(args, wallpaper_socket_path()).await
}

pub async fn dispatch(args: DispatchArgs) -> Result<()> {
    cli::dispatch(args, wallpaper_socket_path()).await
}
