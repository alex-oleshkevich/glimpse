use std::{os::unix::fs::PermissionsExt, path::PathBuf};

use tokio::{
    fs,
    io::{self, AsyncBufReadExt, BufReader},
    net::{UnixListener, UnixStream},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("a daemon is already listening")]
    AlreadyRunning(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct Server {
    listener: UnixListener,
}

impl Server {
    pub async fn bind(socket: &PathBuf) -> Result<Self, ServerError> {
        if let Some(parent) = socket.parent() {
            fs::create_dir_all(parent).await?;
            fs::set_permissions(parent, PermissionsExt::from_mode(0o700)).await?;
        }

        match UnixStream::connect(socket).await {
            Ok(_) => return Err(ServerError::AlreadyRunning(socket.to_owned())),
            Err(err) if err.kind() == io::ErrorKind::ConnectionRefused => {
                fs::remove_file(socket).await?;
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }

        let listener = UnixListener::bind(socket)?;
        fs::set_permissions(socket, PermissionsExt::from_mode(0o600)).await?;
        Ok(Self { listener })
    }

    pub async fn serve(self, cancel: CancellationToken) {
        let mut clients = JoinSet::new();

        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                accepted = self.listener.accept() => match accepted {
                    Ok((stream, _)) => {
                        clients.spawn(serve_client(stream, cancel.child_token()));
                    },
                    Err(err) => tracing::warn!(%err, "failed to accept a client"),
                },
                Some(Err(err)) = clients.join_next(), if !clients.is_empty() => {
                    tracing::warn!(%err, "client task failed");
                }
            }
        }
        while clients.join_next().await.is_some() {}
    }
}

async fn serve_client(stream: UnixStream, cancel: CancellationToken) {
    let mut lines = BufReader::new(stream).lines();

    loop {
        let frame = tokio::select! {
            () = cancel.cancelled() => break,
            frame = lines.next_line() => frame,
        };

        match frame {
            Ok(Some(frame)) => tracing::debug!(bytes = frame.len(), "frame received"),
            Ok(None) => break,
            Err(err) => {
                tracing::warn!(%err, "read failed, closing the connection");
                break;
            }
        }
    }

    tracing::debug!("client disconnected");
}
