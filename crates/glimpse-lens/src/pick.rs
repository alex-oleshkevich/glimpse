use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow};
use glimpse_config::ColorFormat;
use glimpse_widgets::Styles;
use gtk4::glib;

use crate::errors::Cancelled;
use crate::lens::{Outcome, Session, Settings};
use glimpse_wayland::capture::{self, CaptureError};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(3);
const SETUP_GRACE: Duration = Duration::from_secs(1);

pub fn pick(settings: Settings) -> Result<[u8; 3]> {
    let frames = capture_within(CAPTURE_TIMEOUT).context("cannot capture the screen")?;
    gtk4::init().context("cannot initialize GTK")?;
    adw::init().context("cannot initialize libadwaita")?;
    glimpse_widgets::register_resources()?;

    let _styles = Styles::install(adw::ColorScheme::Default);

    let main = glib::MainLoop::new(None, false);
    let outcome = Rc::new(Cell::new(None));
    let session = Session::open(frames, settings, {
        let main = main.clone();
        let outcome = Rc::clone(&outcome);
        move |picked| {
            outcome.set(Some(picked));
            main.quit();
        }
    })
    .map_err(|missing| anyhow!("{missing} was not captured"))?;
    main.run();
    session.close();

    match outcome.get() {
        Some(Outcome::Picked(color)) => Ok(color),
        Some(Outcome::Cancelled) | None => Err(Cancelled.into()),
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

pub fn json(format: ColorFormat, [red, green, blue]: [u8; 3]) -> String {
    serde_json::json!({
        "format": format.name(),
        "value": format.render([red, green, blue]),
        "red": red,
        "green": green,
        "blue": blue,
    })
    .to_string()
}
