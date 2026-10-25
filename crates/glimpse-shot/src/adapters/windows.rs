use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use glimpse_shot::doc::units::{BufferPx, Rect};
use glimpse_shot::ports::{Clipboard, Rect as WindowRect, Window, Windows};
use glimpse_wayland::capture::{self, CaptureError, Frame, Layout, Toplevel};
use glimpse_wayland::selection::{self, Changes};
use gtk4::prelude::*;
use gtk4::{gdk, glib};

use crate::adapters::screens;
use crate::headless::Layout as ShotLayout;
use crate::render::within_frame;

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);
const LATE_FILE_GRACE: Duration = Duration::from_secs(1);
const REASON_CAP: usize = 240;

pub struct CompositorWindows<'a> {
    pub layout: RefCell<ShotLayout>,
    pub clipboard: &'a dyn Clipboard,
    pub restore: bool,
}

impl Windows for CompositorWindows<'_> {
    fn list(&self) -> Result<Vec<Window>, CaptureError> {
        if let Some(fresh) = screens::layout()
            .map_err(|error| CaptureError::Failed(capped(&format!("{error:#}"))))?
        {
            self.layout.replace(fresh);
        }
        Ok(self.layout.borrow().windows.clone())
    }

    fn capture(&self, id: u64) -> Result<Frame, CaptureError> {
        let (window, crop) = {
            let layout = self.layout.borrow();
            let window = layout
                .windows
                .iter()
                .find(|window| window.id == id)
                .cloned()
                .ok_or(CaptureError::NoSuchToplevel)?;
            let crop = visible_crop(&window, &layout);
            (window, crop)
        };
        let connector = window.output.clone().unwrap_or_default();
        let deadline = Instant::now() + CAPTURE_TIMEOUT;
        if matches!(capture::capabilities(deadline), Ok(offered) if offered.toplevels)
            && let Some(identifier) = correlate(&window, &capture::toplevels(deadline)?)
        {
            let frame = capture::toplevel(&identifier, deadline)?;
            return Ok(Frame { connector, ..frame });
        }
        let can_screenshot = screens::session()
            .map_err(|error| CaptureError::Failed(capped(&format!("{error:#}"))))?
            .is_some_and(|link| link.screenshots());
        match crop {
            Some(crop) if window.floating || !can_screenshot => crop_output(&connector, crop),
            _ if can_screenshot => {
                let frame = self.screenshot(&window)?;
                Ok(Frame { connector, ..frame })
            }
            _ => Err(CaptureError::Failed(
                "the window is not on a visible workspace, and Hyprland renders no hidden window"
                    .to_owned(),
            )),
        }
    }
}

impl CompositorWindows<'_> {
    fn screenshot(&self, window: &Window) -> Result<Frame, CaptureError> {
        let link = screens::session()
            .map_err(|error| CaptureError::Failed(capped(&format!("{error:#}"))))?
            .ok_or_else(|| CaptureError::Failed(crate::headless::NO_WINDOWS.to_owned()))?;
        let dir = glimpse_config::runtime_dir()
            .ok_or_else(|| CaptureError::Failed("XDG_RUNTIME_DIR is not set".to_owned()))?;
        std::fs::create_dir_all(&dir).map_err(|error| failed(&dir, error))?;
        let path = dir.join(format!(
            "shot-window-{}-{}.png",
            std::process::id(),
            window.id
        ));
        let started = Instant::now();
        let changes = self.restore.then(Changes::start);
        let before = self.restore.then(|| self.clipboard.snapshot());
        tracing::debug!(elapsed = ?started.elapsed(), "window capture: clipboard read");
        let written = link
            .screenshot(window.id, &path)
            .map_err(CaptureError::Failed);
        tracing::debug!(elapsed = ?started.elapsed(), "window capture: niri wrote the file");
        let late = written.is_err();
        let frame = written.and_then(|()| decode(&path));
        tracing::debug!(elapsed = ?started.elapsed(), "window capture: decoded");
        let _ = std::fs::remove_file(&path);
        if late {
            std::thread::sleep(LATE_FILE_GRACE);
            let _ = std::fs::remove_file(&path);
        }
        if let (Some(before), Some(changes)) = (before, changes) {
            self.put_back(before, changes);
        }
        tracing::debug!(elapsed = ?started.elapsed(), "window capture: clipboard put back");
        frame
    }

    fn put_back(&self, before: io::Result<selection::Snapshot>, changes: io::Result<Changes>) {
        let (before, changes) = match (before, changes) {
            (Ok(before), Ok(changes)) => (before, changes),
            (Err(error), _) | (_, Err(error)) => {
                tracing::warn!("Clipboard replaced: it could not be read beforehand: {error}");
                return;
            }
        };
        if !restore(&changes.since()) {
            tracing::warn!("Clipboard left as it is: something else copied during the capture");
            return;
        }
        if let Some((mime, reason)) = before.skipped.first() {
            tracing::warn!("Clipboard replaced: {mime} was not kept ({reason:?})");
            return;
        }
        if before.entries.is_empty() {
            tracing::warn!(
                "Clipboard replaced: it was empty before the capture, and is left as niri set it"
            );
            return;
        }
        if let Err(reason) = self.clipboard.copy(before.entries) {
            tracing::warn!("Clipboard replaced: it could not be put back: {reason}");
        }
    }
}

fn failed(path: &Path, error: impl std::fmt::Display) -> CaptureError {
    CaptureError::Failed(format!("{}: {error}", path.display()))
}

fn capped(text: &str) -> String {
    glimpse_utils::text::clean(text, REASON_CAP)
}

pub fn decode(path: &Path) -> Result<Frame, CaptureError> {
    let texture = gdk::Texture::from_filename(path).map_err(|error| failed(path, error))?;
    Ok(frame_of(&texture))
}

pub fn frame_of(texture: &gdk::Texture) -> Frame {
    let mut downloader = gdk::TextureDownloader::new(texture);
    downloader.set_format(gdk::MemoryFormat::R8g8b8a8);
    let (bytes, stride) = downloader.download_bytes();
    let (width, height) = (texture.width() as usize, texture.height() as usize);
    let mut pixels = Vec::with_capacity(width * height * 4);
    for row in bytes.chunks(stride).take(height) {
        pixels.extend_from_slice(&row[..(width * 4).min(row.len())]);
    }
    Frame {
        connector: String::new(),
        width: width as u32,
        height: height as u32,
        pixels: glib::Bytes::from_owned(pixels),
        layout: Layout::Rgbx,
        alpha: true,
    }
}

fn crop_output(connector: &str, crop: Rect<BufferPx>) -> Result<Frame, CaptureError> {
    let frame = capture::output(connector, Instant::now() + CAPTURE_TIMEOUT, false)?;
    Ok(cropped(&frame, crop))
}

pub fn restore(changes: &[Vec<String>]) -> bool {
    matches!(changes, [only] if only == &["image/png"])
}

const APP_ID_CAP: usize = 128;
const TITLE_CAP: usize = 512;

fn cleaned(text: Option<&str>, cap: usize) -> Option<String> {
    let text: String = text?
        .chars()
        .filter(|character| !glimpse_utils::hostile(*character))
        .take(cap)
        .collect();
    (!text.is_empty()).then_some(text)
}

pub fn correlate(window: &Window, toplevels: &[Toplevel]) -> Option<String> {
    let app_id = cleaned(window.app_id.as_deref(), APP_ID_CAP);
    let title = cleaned(window.title.as_deref(), TITLE_CAP);
    let mut matching = toplevels.iter().filter(|toplevel| {
        cleaned(toplevel.app_id.as_deref(), APP_ID_CAP) == app_id
            && cleaned(toplevel.title.as_deref(), TITLE_CAP) == title
    });
    match (matching.next(), matching.next()) {
        (Some(only), None) => Some(only.identifier.clone()),
        _ => None,
    }
}

pub fn visible_crop(window: &Window, layout: &ShotLayout) -> Option<Rect<BufferPx>> {
    let rect = window.rect?;
    let visible = layout
        .workspaces
        .iter()
        .any(|workspace| window.workspace == Some(workspace.id.0) && workspace.active);
    let logical = layout
        .outputs
        .iter()
        .find(|output| Some(&output.connector) == window.output.as_ref())?
        .logical?;
    let local = WindowRect {
        x: rect.x - f64::from(logical.x),
        y: rect.y - f64::from(logical.y),
        width: rect.width,
        height: rect.height,
    };
    visible.then(|| buffer_crop(local, logical.scale))
}

pub fn buffer_crop(rect: WindowRect, scale: f64) -> Rect<BufferPx> {
    let to_buffer = |logical: f64| (logical * scale).round() as i32;
    let (x0, y0) = (to_buffer(rect.x), to_buffer(rect.y));
    let (x1, y1) = (
        to_buffer(rect.x + rect.width),
        to_buffer(rect.y + rect.height),
    );
    Rect {
        x: BufferPx(x0),
        y: BufferPx(y0),
        width: BufferPx(x1 - x0),
        height: BufferPx(y1 - y0),
    }
}

pub fn cropped(frame: &Frame, crop: Rect<BufferPx>) -> Frame {
    let crop = within_frame(crop, frame);
    let (x, y) = (crop.x.0 as usize, crop.y.0 as usize);
    let width = (crop.width.0 as usize).min(frame.width as usize - x.min(frame.width as usize));
    let height = (crop.height.0 as usize).min(frame.height as usize - y.min(frame.height as usize));
    let stride = frame.width as usize * 4;
    let pixels: Vec<u8> = (y..y + height)
        .flat_map(|row| {
            let start = row * stride + x * 4;
            frame.pixels[start..start + width * 4].iter().copied()
        })
        .collect();
    Frame {
        connector: frame.connector.clone(),
        width: width as u32,
        height: height as u32,
        pixels: glib::Bytes::from_owned(pixels),
        layout: frame.layout,
        alpha: frame.alpha,
    }
}

#[cfg(test)]
mod tests;
