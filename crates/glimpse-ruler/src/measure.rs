use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow};
use glimpse_config::{
    ColorScheme, Config, DARK_STYLESHEET, PANEL_STYLESHEET, stylesheet, user_dark_stylesheet,
    user_stylesheet,
};
use glimpse_widgets::{Sheets, Styles};
use gtk4::glib;

use crate::capture::{self, CaptureError};
use crate::session::{Outcome, Segment, Session, Settings};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(3);
const SETUP_GRACE: Duration = Duration::from_secs(1);

pub fn measure(document: &Config, settings: Settings) -> Result<Vec<Segment>> {
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
    styles.set_animation_speed(appearance.animation_speed);

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
        let _ = sender.send(capture::capture(deadline));
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

    const EXAMPLE: Segment = Segment {
        from: (412, 268),
        to: (626, 364),
    };

    #[test]
    fn an_output_going_away_discards_what_was_already_confirmed() {
        assert!(confirmed(Some(Outcome::Invalidated)).is_empty());
        assert!(confirmed(None).is_empty());
        assert_eq!(
            confirmed(Some(Outcome::Ended(vec![EXAMPLE]))),
            vec![EXAMPLE]
        );
    }

    #[test]
    fn a_session_with_nothing_confirmed_prints_nothing() {
        assert_eq!(ndjson(&[]), None);
    }

    #[test]
    fn each_segment_is_one_flat_line_in_confirmation_order() {
        let back = Segment {
            from: (626, 364),
            to: (600, 100),
        };
        let text = ndjson(&[EXAMPLE, back]).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();

        assert!(text.ends_with('\n'));
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            serde_json::json!({
                "from_x": 412,
                "from_y": 268,
                "to_x": 626,
                "to_y": 364,
                "dx": 214,
                "dy": 96,
                "distance": 214f64.hypot(96.0),
                "angle": 96f64.atan2(214.0).to_degrees(),
            })
        );
        assert_eq!(lines[1]["dx"], -26);
        assert_eq!(lines[1]["dy"], -264);
    }
}
