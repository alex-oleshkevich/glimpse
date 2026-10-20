pub mod app;
mod canvas;
mod overlay;
mod prefs;
mod result;

use gio_unix::DesktopAppInfo;
use glimpse_shot::doc::document::Document;
use glimpse_widgets::roundtrip;
use gtk4::{gdk, gio, glib};

use crate::cli::ScaleMode;
use crate::headless;
use crate::render::{self, Png};

fn encode(
    doc: Option<Document>,
    scale: ScaleMode,
    done: impl FnOnce(Result<Png, String>) + 'static,
) {
    let rendered = doc
        .ok_or_else(|| anyhow::anyhow!("nothing was captured"))
        .and_then(|doc| render::render(&doc, headless::factor(scale, &doc)));
    glib::spawn_future_local(async move {
        let encoded = match rendered {
            Ok(rendered) => gio::spawn_blocking(move || rendered.encode())
                .await
                .map_err(|_| "the screenshot could not be encoded".to_owned()),
            Err(error) => Err(format!("{error:#}")),
        };
        done(encoded);
    });
}

fn barrier() {
    if let Some(display) = gdk::Display::default()
        && let Err(error) = roundtrip(&display)
    {
        tracing::warn!("no unmap barrier: {error}");
    }
}

fn desktop_entry(app_id: &str) -> Option<DesktopAppInfo> {
    if app_id.is_empty() || app_id.contains('/') || app_id.contains('\0') {
        return None;
    }
    DesktopAppInfo::new(&format!("{app_id}.desktop"))
}
