use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow};
use glimpse_config::{
    ColorFormat, ColorScheme, Config, DARK_STYLESHEET, PANEL_STYLESHEET, stylesheet,
    user_dark_stylesheet, user_stylesheet,
};
use glimpse_widgets::{Sheets, Styles};
use gtk4::glib;

use crate::capture::{self, CaptureError};
use crate::errors::Cancelled;
use crate::lens::{Outcome, Session, Settings};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(3);
const SETUP_GRACE: Duration = Duration::from_secs(1);

pub fn pick(document: &Config, settings: Settings) -> Result<[u8; 3]> {
    let frames = capture_within(CAPTURE_TIMEOUT).context("cannot capture the screen")?;
    gtk4::init().context("cannot initialize GTK")?;
    adw::init().context("cannot initialize libadwaita")?;
    glimpse_widgets::register_resources()?;
    let styles = Styles::install(color_scheme(document.appearance.color_scheme));
    let appearance = &document.appearance;
    styles.load(&Sheets {
        theme: stylesheet(&appearance.theme, PANEL_STYLESHEET),
        theme_dark: stylesheet(&appearance.theme, DARK_STYLESHEET),
        dropin: user_stylesheet(),
        dropin_dark: user_dark_stylesheet(),
    });
    styles.set_variant(&appearance.theme_variant);

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
        let _ = sender.send(capture::capture(deadline));
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

fn color_scheme(scheme: ColorScheme) -> adw::ColorScheme {
    match scheme {
        ColorScheme::Light => adw::ColorScheme::ForceLight,
        ColorScheme::Dark => adw::ColorScheme::ForceDark,
        ColorScheme::Auto => adw::ColorScheme::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_carries_the_notation_the_value_and_every_channel() {
        let parsed: serde_json::Value =
            serde_json::from_str(&json(ColorFormat::Rgb, [224, 86, 63])).unwrap();

        assert_eq!(
            parsed,
            serde_json::json!({
                "format": "rgb",
                "value": "rgb(224 86 63)",
                "red": 224,
                "green": 86,
                "blue": 63,
            })
        );
    }
}
