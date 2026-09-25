use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use glimpse_compositors::Window;
use glimpse_wayland::capture::{CaptureError, Frame};
use glimpse_wayland::selection::{Entry, Snapshot};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub summary: String,
    pub body: String,
    pub image: Option<PathBuf>,
    pub actions: bool,
}

pub trait Screens {
    fn current(&self) -> Result<String, CaptureError>;
    fn capture(&self, name: &str, cursor: bool) -> Result<Frame, CaptureError>;
}

pub trait Windows {
    fn list(&self) -> Result<Vec<Window>, CaptureError>;
    fn capture(&self, id: u64) -> Result<Frame, CaptureError>;
}

pub trait Clipboard {
    fn copy(&self, entries: Vec<Entry>) -> Result<(), String>;
    fn snapshot(&self) -> io::Result<Snapshot>;
}

pub trait Notifier {
    fn notify(&self, notice: Notice) -> Result<(), String>;
    fn closed(&self) -> impl Future<Output = ()>;
}

pub trait Files {
    fn write_new(
        &self,
        dir: &Path,
        name: &str,
        own: Option<&Path>,
        bytes: &[u8],
    ) -> io::Result<PathBuf>;
}

pub trait Clock {
    fn now(&self) -> Instant;
}
