use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use glimpse_compositors::{Compositor, Rect as WindowRect, Snapshot, Window};
use glimpse_shot::doc::units::{BufferPx, Rect};
use glimpse_shot::ports::{Clipboard, Windows};
use glimpse_wayland::capture::{self, CaptureError, Frame, Toplevel};
use glimpse_wayland::selection::{self, Changes};
use gtk4::prelude::*;
use gtk4::{gdk, glib};

use crate::render::within_frame;

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);
const LATE_FILE_GRACE: Duration = Duration::from_secs(1);

pub struct CompositorWindows<'a> {
    pub compositor: Compositor,
    pub snapshot: Snapshot,
    pub clipboard: &'a dyn Clipboard,
    pub restore: bool,
}

impl Windows for CompositorWindows<'_> {
    fn list(&self) -> Result<Vec<Window>, CaptureError> {
        Ok(self.snapshot.windows.clone())
    }

    fn capture(&self, id: u64) -> Result<Frame, CaptureError> {
        let window = self
            .snapshot
            .windows
            .iter()
            .find(|window| window.id.0 == id)
            .ok_or(CaptureError::NoSuchToplevel)?;
        let connector = window.output.clone().unwrap_or_default();
        let deadline = Instant::now() + CAPTURE_TIMEOUT;
        if matches!(capture::capabilities(deadline), Ok(offered) if offered.toplevels)
            && let Some(identifier) = correlate(window, &capture::toplevels(deadline)?)
        {
            let frame = capture::toplevel(&identifier, deadline)?;
            return Ok(Frame { connector, ..frame });
        }
        let visible = visible_crop(window, &self.snapshot);
        match (&self.compositor, visible) {
            (Compositor::Niri(_), Some(crop)) if window.is_floating => {
                crop_output(&connector, crop)
            }
            (Compositor::Niri(_), _) => {
                let frame = self.screenshot(window)?;
                Ok(Frame { connector, ..frame })
            }
            (Compositor::Hyprland(_), Some(crop)) => crop_output(&connector, crop),
            (Compositor::Hyprland(_), None) => Err(CaptureError::Failed(
                "the window is not on a visible workspace, and Hyprland renders no hidden window"
                    .to_owned(),
            )),
            (Compositor::Unsupported, _) => Err(CaptureError::Unsupported),
        }
    }
}

impl CompositorWindows<'_> {
    fn screenshot(&self, window: &Window) -> Result<Frame, CaptureError> {
        let dir = glimpse_config::runtime_dir()
            .ok_or_else(|| CaptureError::Failed("XDG_RUNTIME_DIR is not set".to_owned()))?;
        std::fs::create_dir_all(&dir).map_err(|error| failed(&dir, error))?;
        let path = dir.join(format!(
            "shot-window-{}-{}.png",
            std::process::id(),
            window.id.0
        ));
        let started = Instant::now();
        let changes = self.restore.then(Changes::start);
        let before = self.restore.then(|| self.clipboard.snapshot());
        tracing::debug!(elapsed = ?started.elapsed(), "window capture: clipboard read");
        let written = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| failed(&path, error))
            .and_then(|runtime| {
                runtime
                    .block_on(self.compositor.screenshot_window(window.id, &path))
                    .map_err(|error| CaptureError::Failed(error.to_string()))
            });
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
        .filter(|character| {
            !character.is_control()
                && !matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
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

pub fn visible_crop(window: &Window, snapshot: &Snapshot) -> Option<Rect<BufferPx>> {
    let rect = window.rect?;
    let visible = snapshot
        .workspaces
        .iter()
        .any(|workspace| Some(workspace.id) == window.workspace_id && workspace.is_active);
    let scale = snapshot
        .outputs
        .iter()
        .find(|output| Some(&output.connector) == window.output.as_ref())?
        .logical
        .as_ref()?
        .scale;
    visible.then(|| buffer_crop(rect, scale))
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
        alpha: frame.alpha,
    }
}

#[cfg(test)]
mod tests {
    use glimpse_compositors::{Logical, Output, WindowId, Workspace, WorkspaceId};

    use super::*;

    fn mimes(list: &[&str]) -> Vec<String> {
        list.iter().map(|mime| (*mime).to_owned()).collect()
    }

    #[test]
    fn the_clipboard_is_restored_only_when_niri_alone_replaced_it() {
        assert!(restore(&[mimes(&["image/png"])]));
        assert!(!restore(&[]));
        assert!(!restore(&[mimes(&["image/png"]), mimes(&["image/png"])]));
        assert!(!restore(&[mimes(&["text/plain"]), mimes(&["image/png"])]));
        assert!(!restore(&[mimes(&["text/plain"])]));
        assert!(!restore(&[mimes(&["image/png", "text/plain"])]));
        assert!(!restore(&[mimes(&[])]));
    }

    fn window(app_id: &str, title: &str) -> Window {
        Window {
            id: WindowId(42),
            title: Some(title.to_owned()),
            app_id: Some(app_id.to_owned()),
            pid: None,
            workspace_id: Some(WorkspaceId(8)),
            is_focused: false,
            is_floating: true,
            is_urgent: false,
            layout_order: None,
            size: Some((400, 300)),
            rect: Some(WindowRect {
                x: 952.0,
                y: 568.0,
                width: 400.0,
                height: 300.0,
            }),
            output: Some("eDP-1".to_owned()),
        }
    }

    fn toplevel(identifier: &str, app_id: &str, title: &str) -> Toplevel {
        Toplevel {
            identifier: identifier.to_owned(),
            app_id: Some(app_id.to_owned()),
            title: Some(title.to_owned()),
        }
    }

    #[test]
    fn a_window_matches_a_toplevel_only_when_exactly_one_has_its_app_id_and_title() {
        let wanted = window("firefox", "Docs");
        let one = [
            toplevel("a", "firefox", "Mail"),
            toplevel("b", "firefox", "Docs"),
            toplevel("c", "kitty", "Docs"),
        ];
        assert_eq!(correlate(&wanted, &one), Some("b".to_owned()));

        let two = [
            toplevel("a", "firefox", "Docs"),
            toplevel("b", "firefox", "Docs"),
        ];
        assert_eq!(correlate(&wanted, &two), None);
        assert_eq!(correlate(&wanted, &one[..1]), None);
        assert_eq!(correlate(&wanted, &[]), None);
    }

    #[test]
    fn titles_and_app_ids_are_compared_as_the_compositor_cleans_them() {
        let sanitized = Window {
            app_id: None,
            ..window("unused", "Docs")
        };
        let raw_bidi = Toplevel {
            identifier: "bidi".to_owned(),
            app_id: Some(String::new()),
            title: Some("\u{202e}Docs".to_owned()),
        };
        let clean = Toplevel {
            identifier: "clean".to_owned(),
            app_id: None,
            title: Some("Docs".to_owned()),
        };

        assert_eq!(
            correlate(&sanitized, std::slice::from_ref(&raw_bidi)),
            Some("bidi".to_owned())
        );
        assert_eq!(correlate(&sanitized, &[raw_bidi, clean]), None);
    }

    fn snapshot(active: bool) -> Snapshot {
        Snapshot {
            outputs: vec![Output {
                connector: "eDP-1".to_owned(),
                make: None,
                model: None,
                serial: None,
                description: None,
                logical: Some(Logical {
                    x: -1440,
                    y: 0,
                    width: 2304,
                    height: 1440,
                    scale: 1.25,
                }),
                current_mode: None,
                enabled: true,
                built_in: true,
            }],
            workspaces: vec![Workspace {
                id: WorkspaceId(8),
                idx: Some(1),
                name: None,
                output: Some("eDP-1".to_owned()),
                is_active: active,
                is_focused: active,
                is_urgent: false,
                active_window_id: None,
            }],
            ..Snapshot::default()
        }
    }

    fn buffer(x: i32, y: i32, width: i32, height: i32) -> Rect<BufferPx> {
        Rect {
            x: BufferPx(x),
            y: BufferPx(y),
            width: BufferPx(width),
            height: BufferPx(height),
        }
    }

    #[test]
    fn a_floating_window_is_its_output_relative_rect_times_the_scale() {
        let floating = window("firefox", "Docs");

        assert_eq!(
            visible_crop(&floating, &snapshot(true)),
            Some(buffer(1190, 710, 500, 375))
        );
        assert_eq!(visible_crop(&floating, &snapshot(false)), None);
        let tiled = Window {
            rect: None,
            ..floating
        };
        assert_eq!(visible_crop(&tiled, &snapshot(true)), None);
    }

    #[test]
    fn a_crop_copies_the_rows_it_covers_and_stays_inside_the_frame() {
        let frame = Frame {
            connector: "eDP-1".to_owned(),
            width: 4,
            height: 3,
            pixels: glib::Bytes::from_owned(
                (0u8..12).flat_map(|i| [i, 0, 0, 255]).collect::<Vec<_>>(),
            ),
            alpha: false,
        };
        let reds = |frame: &Frame| frame.pixels.chunks(4).map(|p| p[0]).collect::<Vec<_>>();

        let inner = cropped(&frame, buffer(1, 1, 2, 2));
        assert_eq!((inner.width, inner.height), (2, 2));
        assert_eq!(reds(&inner), [5, 6, 9, 10]);

        let past = cropped(&frame, buffer(3, 2, 10, 10));
        assert_eq!((past.width, past.height), (1, 1));
        assert_eq!(reds(&past), [11]);
    }
}
