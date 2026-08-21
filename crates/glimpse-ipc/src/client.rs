use std::path::PathBuf;

pub struct IPCClient {}

impl IPCClient {
    pub async fn from_socket(socket: PathBuf) -> Self {
        Self {}
    }
}
