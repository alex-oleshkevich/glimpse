use std::sync::Arc;

use glimpse_core::{
    ResolvedWallpaperSpec,
    ipc::{self, IpcHandle, IpcServer, client::NoopCommandHandler, protocol::IpcEvent,
          wallpaper_socket_path},
};
use tokio::sync::broadcast;

pub fn start() -> (IpcHandle, broadcast::Sender<Arc<IpcEvent>>) {
    let tx = ipc::new_event_channel();
    let handle = IpcServer::launch_at(tx.clone(), wallpaper_socket_path(), NoopCommandHandler);
    (handle, tx)
}

pub fn emit_spec_changed(tx: &broadcast::Sender<Arc<IpcEvent>>, spec: &ResolvedWallpaperSpec) {
    let (mode, path) = match &spec.image {
        Some(image) => ("image", image.path.display().to_string()),
        None => ("color", String::new()),
    };
    let mut fields = vec![
        ("mode", mode.to_owned()),
        ("color", spec.color.clone()),
    ];
    if !path.is_empty() {
        fields.push(("path", path));
    }
    ipc::emit(tx, "wallpaper.spec_changed", fields);
}
