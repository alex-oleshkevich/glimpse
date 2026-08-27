use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

/// A stand-in niri: one connection per request, a reply line chosen by `respond`, and any further
/// lines it returns written in order — which is what an `EventStream` subscription looks like.
pub struct FakeNiri {
    _dir: tempfile::TempDir,
    pub socket: PathBuf,
}

impl FakeNiri {
    pub fn spawn(respond: impl Fn(&str) -> Vec<String> + Send + Sync + 'static) -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let socket = dir.path().join("niri.sock");
        let listener = UnixListener::bind(&socket).expect("bind the fake niri socket");
        let respond = Arc::new(respond);

        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let respond = respond.clone();
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stream);
                    let mut request = String::new();
                    if reader.read_line(&mut request).await.is_err() {
                        return;
                    }
                    for line in respond(request.trim()) {
                        if reader
                            .get_mut()
                            .write_all(format!("{line}\n").as_bytes())
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                });
            }
        });

        Self { _dir: dir, socket }
    }

    /// Accepts a connection and drops it without replying.
    pub fn silent() -> Self {
        Self::spawn(|_| Vec::new())
    }
}
