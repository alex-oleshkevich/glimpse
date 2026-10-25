use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use glimpse_wayland::capture::{CaptureError, Frame};
use glimpse_wayland::selection::{Entry, Snapshot};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub id: u64,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub workspace: Option<u64>,
    pub output: Option<String>,
    pub focused: bool,
    pub floating: bool,
    pub rect: Option<Rect>,
}

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
