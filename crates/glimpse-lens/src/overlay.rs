use std::sync::mpsc;
use std::time::{Duration, Instant};

use glimpse_wayland::capture::{self, CaptureError};
use gtk4::gdk;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const SETUP_GRACE: Duration = Duration::from_secs(1);
const PILL_GAP: f64 = 10.0;

pub(crate) fn capture_within(timeout: Duration) -> Result<Vec<capture::Frame>, CaptureError> {
    let deadline = Instant::now() + timeout;
    let (sender, frames) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(capture::outputs(deadline, false));
    });
    frames
        .recv_timeout(timeout + SETUP_GRACE)
        .unwrap_or(Err(CaptureError::TimedOut))
}

pub(crate) fn overlay(monitor: &gdk::Monitor) -> gtk4::Window {
    let window = gtk4::Window::new();
    window.init_layer_shell();
    window.set_namespace(Some("glimpse-lens"));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_exclusive_zone(-1);
    window.set_monitor(Some(monitor));
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window
}

pub(crate) fn pill_origin(
    pointer: (f64, f64),
    radius: f64,
    pill: (f64, f64),
    area: (f64, f64),
) -> (f64, f64) {
    let below = pointer.1 + radius + PILL_GAP;
    let top = match below + pill.1 > area.1 {
        true => pointer.1 - radius - PILL_GAP - pill.1,
        false => below,
    };
    let left = (pointer.0 - pill.0 / 2.0).clamp(0.0, (area.0 - pill.0).max(0.0));
    (left.round(), top.max(0.0).round())
}
