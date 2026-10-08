use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow};
use glimpse_widgets::Styles;
use gtk4::glib;

use crate::session::{Outcome, Segment, Session, Settings};
use glimpse_wayland::capture::{self, CaptureError};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(3);
const SETUP_GRACE: Duration = Duration::from_secs(1);

pub fn measure(settings: Settings) -> Result<Vec<Segment>> {
    let frames = capture_within(CAPTURE_TIMEOUT).context("cannot capture the screen")?;
    gtk4::init().context("cannot initialize GTK")?;
    adw::init().context("cannot initialize libadwaita")?;
    glimpse_widgets::register_resources()?;

    let _styles = Styles::install(adw::ColorScheme::Default);

    let main = glib::MainLoop::new(None, false);
    let outcome = Rc::new(RefCell::new(None));
    let session = Session::open(frames, settings, {
        let main = main.clone();
        let outcome = Rc::clone(&outcome);
        move |ended| {
            outcome.replace(Some(ended));
            main.quit();
        }
    })
    .map_err(|missing| anyhow!("{missing} was not captured"))?;
    main.run();
    session.close();

    Ok(confirmed(outcome.take()))
}

fn confirmed(outcome: Option<Outcome>) -> Vec<Segment> {
    match outcome {
        Some(Outcome::Ended(segments)) => segments,
        Some(Outcome::Invalidated) => {
            tracing::info!("an output went away, so the session is discarded");
            Vec::new()
        }
        None => Vec::new(),
    }
}

fn capture_within(timeout: Duration) -> Result<Vec<capture::Frame>, CaptureError> {
    let deadline = Instant::now() + timeout;
    let (sender, frames) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(capture::outputs(deadline, false));
    });
    frames
        .recv_timeout(timeout + SETUP_GRACE)
        .unwrap_or(Err(CaptureError::TimedOut))
}

pub fn ndjson(segments: &[Segment]) -> Option<String> {
    if segments.is_empty() {
        return None;
    }
    Some(
        segments
            .iter()
            .map(|segment| json(*segment) + "\n")
            .collect(),
    )
}

fn json(segment: Segment) -> String {
    let (dx, dy) = segment.delta();
    serde_json::json!({
        "from_x": segment.from.0,
        "from_y": segment.from.1,
        "to_x": segment.to.0,
        "to_y": segment.to.1,
        "dx": dx,
        "dy": dy,
        "distance": segment.distance(),
        "angle": segment.angle(),
    })
    .to_string()
}

#[cfg(test)]
mod tests;
