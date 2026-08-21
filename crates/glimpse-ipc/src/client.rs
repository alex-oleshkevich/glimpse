use std::path::PathBuf;
use tokio::net::UnixStream;

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("no daemon listening at {path}")]
    NotListening {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("daemon speaks protocol version {daemon}, we speak {ours}")]
    ProtocolMismatch { daemon: u32, ours: u32 },
}

pub struct Client {
    stream: UnixStream,
}

impl Client {
    pub async fn connect(socket: &PathBuf) -> Result<Self, ConnectError> {
        let stream =
            UnixStream::connect(socket)
                .await
                .map_err(|source| ConnectError::NotListening {
                    path: socket.clone(),
                    source,
                })?;

        Ok(Self { stream })
    }
}
