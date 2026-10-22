use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum CompositorError {
    #[error("no supported compositor: {0}")]
    Unsupported(&'static str),
    #[error("this compositor cannot {0}")]
    Unavailable(&'static str),
    #[error("compositor socket {}: {error}", path.display())]
    Connect {
        path: PathBuf,
        error: std::io::Error,
    },
    #[error("the compositor closed the connection before replying")]
    Closed,
    #[error("the compositor refused the request: {0}")]
    Refused(String),
    #[error("undecodable reply: {0}")]
    Protocol(String),
    #[error("the compositor wrote no file within {0:?}")]
    NoFile(Duration),
    #[error("not an absolute UTF-8 file path: {}", .0.display())]
    InvalidPath(PathBuf),
    #[error("watching {}: {error}", path.display())]
    Watch {
        path: PathBuf,
        error: std::io::Error,
    },
}

impl CompositorError {
    pub(crate) fn connect(path: impl Into<PathBuf>, error: std::io::Error) -> Self {
        Self::Connect {
            path: path.into(),
            error,
        }
    }

    pub(crate) fn protocol(context: impl std::fmt::Display) -> Self {
        Self::Protocol(context.to_string())
    }
}
