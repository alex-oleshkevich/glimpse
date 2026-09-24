use std::cell::Cell;
use std::rc::Rc;

use glimpse_config::ColorFormat;
use glimpse_widgets::{Lens, Swatch, rgba, zoomed};
use gtk4::{gdk, glib, prelude::*};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::capture::Frame;

const DEFAULT_ZOOM: u32 = 8;
const PILL_GAP: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub format: ColorFormat,
    pub radius: f32,
    pub max_zoom: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Picked([u8; 3]),
    Cancelled,
}

struct Surface {
    monitor: gdk::Monitor,
    window: gtk4::Window,
    lens: Lens,
    layer: gtk4::Fixed,
    pill: gtk4::Box,
    swatch: Swatch,
    value: gtk4::Label,
    size: Cell<(f64, f64)>,
    frame: Frame,
}

pub struct Session {
    surfaces: Rc<Vec<Surface>>,
    invalidated: Vec<(gdk::Monitor, glib::SignalHandlerId)>,
}

impl Session {
    pub fn open(
        mut frames: Vec<Frame>,
        settings: Settings,
        done: impl Fn(Outcome) + 'static,
    ) -> Result<Self, String> {
        let display = gdk::Display::default().ok_or_else(|| "the display".to_owned())?;
        let zoom = Rc::new(Cell::new(DEFAULT_ZOOM.min(settings.max_zoom.max(1))));
        let mut surfaces: Vec<Surface> = Vec::new();
        for monitor in display.monitors().iter::<gdk::Monitor>().flatten() {
            let connector = monitor
                .connector()
                .map(|name| name.to_string())
                .unwrap_or_default();
            let Some(index) = frames.iter().position(|frame| frame.connector == connector) else {
                for surface in &surfaces {
                    surface.window.destroy();
                }
                return Err(format!("output {connector:?}"));
            };
            surfaces.push(surface(
                &monitor,
                frames.swap_remove(index),
                zoom.get(),
                settings.radius,
            ));
        }
        if surfaces.is_empty() {
            return Err("any output".to_owned());
        }

        let surfaces = Rc::new(surfaces);
        let finished = Cell::new(false);
        let finish: Rc<dyn Fn(Outcome)> = Rc::new(move |outcome| {
            if !finished.replace(true) {
                done(outcome);
            }
        });

        for index in 0..surfaces.len() {
            connect(&surfaces, index, zoom.clone(), settings, finish.clone());
        }
        let mut invalidated = Vec::new();
        for surface in surfaces.iter() {
            let handler = surface.monitor.connect_invalidate({
                let finish = finish.clone();
                move |_| finish(Outcome::Cancelled)
            });
            invalidated.push((surface.monitor.clone(), handler));
            surface.window.present();
        }
        Ok(Self {
            surfaces,
            invalidated,
        })
    }

    pub fn close(self) {
        for (monitor, handler) in self.invalidated {
            monitor.disconnect(handler);
        }
        for surface in self.surfaces.iter() {
            surface.window.destroy();
        }
    }
}

fn surface(monitor: &gdk::Monitor, frame: Frame, zoom: u32, radius: f32) -> Surface {
    let window = gtk4::Window::new();
    window.init_layer_shell();
    window.set_namespace(Some("glimpse-picker"));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_exclusive_zone(-1);
    window.set_monitor(Some(monitor));
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.add_css_class("picker");
    window.set_cursor(gdk::Cursor::from_name("none", None).as_ref());

    let texture: gdk::Texture = gdk::MemoryTexture::new(
        frame.width as i32,
        frame.height as i32,
        gdk::MemoryFormat::R8g8b8x8,
        &frame.pixels,
        frame.width as usize * 4,
    )
    .upcast();
    let lens = Lens::new(&texture, (frame.width, frame.height), zoom, radius);
    let swatch = Swatch::default();
    swatch.add_css_class("picker__swatch");
    swatch.set_valign(gtk4::Align::Center);
    let value = gtk4::Label::new(None);
    value.add_css_class("picker__value");
    let pill = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    pill.add_css_class("picker__pill");
    pill.set_visible(false);
    pill.append(&swatch);
    pill.append(&value);
    let layer = gtk4::Fixed::new();
    layer.set_can_target(false);
    layer.put(&pill, 0.0, 0.0);

    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&lens));
    overlay.add_overlay(&layer);
    window.set_child(Some(&overlay));

    Surface {
        monitor: monitor.clone(),
        window,
        lens,
        layer,
        pill,
        swatch,
        value,
        size: Cell::new((0.0, 0.0)),
        frame,
    }
}

fn dress(surface: &Surface, settings: Settings) {
    let Some((x, y)) = surface.lens.pointer() else {
        surface.pill.set_visible(false);
        return;
    };
    let (column, row) = surface.lens.pixel();
    let color = surface.frame.pixel(column, row);
    surface.swatch.set_color(Some(&rgba(color)));
    let text = settings.format.render(color);
    surface.pill.set_visible(true);
    if surface.value.text() != text {
        surface.value.set_text(&text);
        let (_, width, _, _) = surface.pill.measure(gtk4::Orientation::Horizontal, -1);
        let (_, height, _, _) = surface.pill.measure(gtk4::Orientation::Vertical, width);
        surface.size.set((f64::from(width), f64::from(height)));
    }
    let (left, top) = pill_origin(
        (x, y),
        f64::from(settings.radius),
        surface.size.get(),
        (
            f64::from(surface.lens.width()),
            f64::from(surface.lens.height()),
        ),
    );
    surface.layer.move_(&surface.pill, left, top);
}

fn pill_origin(pointer: (f64, f64), radius: f64, pill: (f64, f64), area: (f64, f64)) -> (f64, f64) {
    let below = pointer.1 + radius + PILL_GAP;
    let top = match below + pill.1 > area.1 {
        true => pointer.1 - radius - PILL_GAP - pill.1,
        false => below,
    };
    let left = (pointer.0 - pill.0 / 2.0).clamp(0.0, (area.0 - pill.0).max(0.0));
    (left.round(), top.max(0.0).round())
}

fn connect(
    surfaces: &Rc<Vec<Surface>>,
    index: usize,
    zoom: Rc<Cell<u32>>,
    settings: Settings,
    finish: Rc<dyn Fn(Outcome)>,
) {
    let surface = &surfaces[index];
    let weak = Rc::downgrade(surfaces);

    let motion = gtk4::EventControllerMotion::new();
    motion.connect_enter({
        let weak = weak.clone();
        move |_, x, y| moved(&weak, index, x, y, settings)
    });
    motion.connect_motion({
        let weak = weak.clone();
        move |_, x, y| moved(&weak, index, x, y, settings)
    });
    motion.connect_leave({
        let weak = weak.clone();
        move |_| {
            if let Some(surfaces) = weak.upgrade() {
                surfaces[index].lens.leave();
                dress(&surfaces[index], settings);
            }
        }
    });
    surface.lens.add_controller(motion);

    let click = gtk4::GestureClick::new();
    click.set_button(0);
    click.connect_pressed({
        let weak = weak.clone();
        let finish = finish.clone();
        move |gesture, _, x, y| {
            let Some(surfaces) = weak.upgrade() else {
                return;
            };
            match gesture.current_button() {
                gdk::BUTTON_PRIMARY => {
                    let surface = &surfaces[index];
                    if !surface.lens.hovered() {
                        surface.lens.point(x, y);
                    }
                    let (column, row) = surface.lens.pixel();
                    finish(Outcome::Picked(surface.frame.pixel(column, row)));
                }
                gdk::BUTTON_SECONDARY => finish(Outcome::Cancelled),
                _ => {}
            }
        }
    });
    surface.lens.add_controller(click);

    let scroll = gtk4::EventControllerScroll::new(
        gtk4::EventControllerScrollFlags::VERTICAL | gtk4::EventControllerScrollFlags::DISCRETE,
    );
    scroll.connect_scroll({
        let weak = weak.clone();
        move |_, _, dy| {
            let next = zoomed(zoom.get(), dy, settings.max_zoom);
            zoom.set(next);
            if let Some(surfaces) = weak.upgrade() {
                for surface in surfaces.iter() {
                    surface.lens.set_zoom(next);
                }
            }
            glib::Propagation::Stop
        }
    });
    surface.lens.add_controller(scroll);

    let keys = gtk4::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, _| {
        let Some(surfaces) = weak.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let Some(surface) = surfaces.iter().find(|surface| surface.lens.hovered()) else {
            if key == gdk::Key::Escape {
                finish(Outcome::Cancelled);
            }
            return glib::Propagation::Stop;
        };
        let step = |dx, dy| {
            surface.lens.nudge(dx, dy);
            dress(surface, settings);
        };
        match key {
            gdk::Key::Escape => finish(Outcome::Cancelled),
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space => {
                let (column, row) = surface.lens.pixel();
                finish(Outcome::Picked(surface.frame.pixel(column, row)));
            }
            gdk::Key::Left => step(-1, 0),
            gdk::Key::Right => step(1, 0),
            gdk::Key::Up => step(0, -1),
            gdk::Key::Down => step(0, 1),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    });
    surface.window.add_controller(keys);
}

fn moved(weak: &std::rc::Weak<Vec<Surface>>, index: usize, x: f64, y: f64, settings: Settings) {
    let Some(surfaces) = weak.upgrade() else {
        return;
    };
    for (other, surface) in surfaces.iter().enumerate() {
        if other != index && surface.lens.hovered() {
            surface.lens.leave();
            dress(surface, settings);
        }
    }
    surfaces[index].lens.point(x, y);
    dress(&surfaces[index], settings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_value_sits_under_the_lens_and_flips_above_it_near_the_bottom_edge() {
        let area = (1000.0, 800.0);
        let pill = (120.0, 34.0);

        assert_eq!(
            pill_origin((500.0, 300.0), 88.0, pill, area),
            (440.0, 398.0)
        );
        assert_eq!(
            pill_origin((500.0, 780.0), 88.0, pill, area),
            (440.0, 648.0)
        );
        assert_eq!(pill_origin((10.0, 300.0), 88.0, pill, area), (0.0, 398.0));
        assert_eq!(
            pill_origin((995.0, 300.0), 88.0, pill, area),
            (880.0, 398.0)
        );
        assert_eq!(
            pill_origin((500.0, 300.0), 106.0, pill, area),
            (440.0, 416.0)
        );
    }
}
