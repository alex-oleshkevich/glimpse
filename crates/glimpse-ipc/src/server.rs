use std::{io, os::unix::fs::PermissionsExt, path::PathBuf};

use tokio::{
    fs,
    net::{UnixListener, UnixStream},
};

#[derive(Debug, thiserror::Error)]
pub enum BindError {
    #[error("a daemon is already listening at {0}")]
    AlreadyRunning(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct Server {}

impl Server {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn listen(socket: &PathBuf) -> Result<UnixListener, BindError> {
        if let Some(parent) = socket.parent() {
            fs::create_dir_all(parent).await?;
            fs::set_permissions(parent, PermissionsExt::from_mode(0o700)).await?;
        }

        match UnixStream::connect(socket).await {
            Ok(_) => return Err(BindError::AlreadyRunning(socket.to_owned())),
            Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                fs::remove_file(socket).await?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }

        let listener = UnixListener::bind(socket)?;
        fs::set_permissions(socket, PermissionsExt::from_mode(0o600)).await?;
        Ok(listener)
    }
}
