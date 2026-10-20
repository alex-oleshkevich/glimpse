use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow};
use glimpse_wayland::capture::{self, CaptureError, Frame, Layout};
use glimpse_widgets::{Lens, Styles};
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::{MAX_RADIUS, MIN_RADIUS};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(3);
const RADIUS_STEP: f32 = 1.15;
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

pub(crate) fn resized(radius: f32, notches: f64) -> f32 {
    let next = if notches < 0.0 {
        (radius * RADIUS_STEP).max(radius + 4.0)
    } else if notches > 0.0 {
        (radius / RADIUS_STEP).min(radius - 4.0)
    } else {
        radius
    };
    next.clamp(MIN_RADIUS as f32, MAX_RADIUS as f32)
}

pub(crate) fn texture(frame: &Frame) -> gdk::Texture {
    gdk::MemoryTexture::new(
        frame.width as i32,
        frame.height as i32,
        match frame.layout {
            Layout::Rgbx => gdk::MemoryFormat::R8g8b8x8,
            Layout::Bgrx => gdk::MemoryFormat::B8g8r8x8,
        },
        &frame.pixels,
        frame.width as usize * 4,
    )
    .upcast()
}

pub(crate) fn connectors(display: &gdk::Display) -> Vec<(gdk::Monitor, String)> {
    display
        .monitors()
        .iter::<gdk::Monitor>()
        .flatten()
        .map(|monitor| {
            let connector = monitor
                .connector()
                .map(|name| name.to_string())
                .unwrap_or_default();
            (monitor, connector)
        })
        .collect()
}

pub(crate) fn matched<M>(
    monitors: Vec<(M, String)>,
    mut frames: Vec<Frame>,
) -> Result<Vec<(M, Frame)>, String> {
    if monitors.is_empty() {
        return Err("any output".to_owned());
    }
    monitors
        .into_iter()
        .map(|(monitor, connector)| {
            match frames.iter().position(|frame| frame.connector == connector) {
                Some(index) => Ok((monitor, frames.swap_remove(index))),
                None => Err(format!("output {connector:?}")),
            }
        })
        .collect()
}

pub(crate) fn run<S, O: 'static>(
    open: impl FnOnce(Vec<Frame>, Box<dyn Fn(O)>) -> Result<S, String>,
    close: impl FnOnce(S),
) -> Result<Option<O>> {
    let frames = capture_within(CAPTURE_TIMEOUT).context("cannot capture the screen")?;
    gtk4::init().context("cannot initialize GTK")?;
    adw::init().context("cannot initialize libadwaita")?;
    glimpse_widgets::register_resources()?;

    let _styles = Styles::install(adw::ColorScheme::Default);

    let main = glib::MainLoop::new(None, false);
    let outcome = Rc::new(RefCell::new(None));
    let session = open(frames, {
        let main = main.clone();
        let outcome = Rc::clone(&outcome);
        Box::new(move |ended| {
            outcome.replace(Some(ended));
            main.quit();
        })
    })
    .map_err(|missing| anyhow!("{missing} was not captured"))?;
    main.run();
    close(session);

    Ok(outcome.take())
}

pub(crate) struct Watch {
    invalidated: Vec<(gdk::Monitor, glib::SignalHandlerId)>,
    added: (gio::ListModel, glib::SignalHandlerId),
}

impl Watch {
    pub(crate) fn new<'a>(
        display: &gdk::Display,
        monitors: impl Iterator<Item = &'a gdk::Monitor>,
        ended: Rc<dyn Fn()>,
    ) -> Self {
        let invalidated = monitors
            .map(|monitor| {
                let ended = ended.clone();
                (
                    monitor.clone(),
                    monitor.connect_invalidate(move |_| ended()),
                )
            })
            .collect();
        let added = on_output_added(&display.monitors(), move || ended());
        Self { invalidated, added }
    }

    pub(crate) fn close(self) {
        self.added.0.disconnect(self.added.1);
        for (monitor, handler) in self.invalidated {
            monitor.disconnect(handler);
        }
    }
}

pub(crate) fn place_pill(
    layer: &gtk4::Fixed,
    pill: &gtk4::Box,
    size: &Cell<(f64, f64)>,
    remeasure: bool,
    lens: &Lens,
    pointer: (f64, f64),
    radius: f32,
) {
    if remeasure {
        let (_, width, _, _) = pill.measure(gtk4::Orientation::Horizontal, -1);
        let (_, height, _, _) = pill.measure(gtk4::Orientation::Vertical, width);
        size.set((f64::from(width), f64::from(height)));
    }
    let (left, top) = pill_origin(
        pointer,
        f64::from(radius),
        size.get(),
        (f64::from(lens.width()), f64::from(lens.height())),
    );
    layer.move_(pill, left, top);
}

pub(crate) fn on_output_added(
    monitors: &gio::ListModel,
    ended: impl Fn() + 'static,
) -> (gio::ListModel, glib::SignalHandlerId) {
    let handler = monitors.connect_items_changed(move |_, _, _, added| {
        if added > 0 {
            ended();
        }
    });
    (monitors.clone(), handler)
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

#[cfg(test)]
mod tests;
