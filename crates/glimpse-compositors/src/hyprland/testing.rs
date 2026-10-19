use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

pub struct FakeHyprland {
    _dir: tempfile::TempDir,
    pub dir: PathBuf,
}

impl FakeHyprland {
    pub fn spawn(
        respond: impl Fn(&str) -> String + Send + Sync + 'static,
        events: Vec<String>,
    ) -> Self {
        Self::spawn_raw(
            move |command| respond(command).into_bytes(),
            events
                .into_iter()
                .map(|line| format!("{line}\n").into_bytes())
                .collect(),
        )
    }

    pub fn spawn_raw(
        respond: impl Fn(&str) -> Vec<u8> + Send + Sync + 'static,
        events: Vec<Vec<u8>>,
    ) -> Self {
        let temp = tempfile::tempdir().expect("a temporary directory");
        let dir = temp.path().to_path_buf();

        let control =
            UnixListener::bind(dir.join(".socket.sock")).expect("bind the control socket");
        let respond = Arc::new(respond);
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = control.accept().await {
                let respond = respond.clone();
                tokio::spawn(async move {
                    let mut command = vec![0_u8; 4096];
                    let Ok(read) = stream.read(&mut command).await else {
                        return;
                    };
                    let command = String::from_utf8_lossy(&command[..read]).into_owned();
                    let _ = stream.write_all(&respond(command.trim())).await;
                });
            }
        });

        let stream_socket =
            UnixListener::bind(dir.join(".socket2.sock")).expect("bind the event socket");
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = stream_socket.accept().await {
                let events = events.clone();
                tokio::spawn(async move {
                    for line in events {
                        if stream.write_all(&line).await.is_err() {
                            return;
                        }
                    }
                });
            }
        });

        Self { _dir: temp, dir }
    }
}
