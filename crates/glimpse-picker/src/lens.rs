use std::cell::{Cell, RefCell};
use std::rc::Rc;

use glimpse_config::ColorFormat;
use glimpse_widgets::{Swatch, rgba};
use gtk4::{gdk, glib, graphene, gsk, prelude::*, subclass::prelude::*};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::capture::Frame;

const DEFAULT_ZOOM: u32 = 8;
const ZOOM_STEP: f64 = 1.25;
const RING: f32 = 2.0;
const GRID_FROM_ZOOM: u32 = 6;
const CROSSHAIR_MIN: f32 = 7.0;
const PILL_GAP: f64 = 10.0;

fn pixel_at(x: f64, y: f64, logical: (f64, f64), buffer: (u32, u32)) -> (u32, u32) {
    let column = (x / logical.0 * f64::from(buffer.0)).floor();
    let row = (y / logical.1 * f64::from(buffer.1)).floor();
    let clamp = |value: f64, size: u32| value.clamp(0.0, f64::from(size.saturating_sub(1))) as u32;
    (clamp(column, buffer.0), clamp(row, buffer.1))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub format: ColorFormat,
    pub radius: f32,
    pub max_zoom: u32,
}

fn zoomed(zoom: u32, notches: f64, max: u32) -> u32 {
    let current = f64::from(zoom);
    let next = if notches < 0.0 {
        (current * ZOOM_STEP).round().max(current + 1.0)
    } else if notches > 0.0 {
        (current / ZOOM_STEP).round().min(current - 1.0)
    } else {
        current
    };
    (next.max(1.0) as u32).clamp(1, max.max(1))
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Lens {
        pub texture: RefCell<Option<gdk::Texture>>,
        pub buffer: Cell<(u32, u32)>,
        pub pointer: Cell<Option<(f64, f64)>>,
        pub nudge: Cell<(i32, i32)>,
        pub zoom: Cell<u32>,
        pub radius: Cell<f32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Lens {
        const NAME: &'static str = "PickerLens";
        type Type = super::Lens;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("picker-lens");
        }
    }

    impl ObjectImpl for Lens {}

    impl WidgetImpl for Lens {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let Some(texture) = self.texture.borrow().clone() else {
                return;
            };
            let (width, height) = (widget.width() as f32, widget.height() as f32);
            snapshot.append_texture(&texture, &graphene::Rect::new(0.0, 0.0, width, height));

            let Some((x, y)) = self.pointer.get() else {
                return;
            };
            let (buffer_width, buffer_height) = self.buffer.get();
            let (column, row) = widget.pixel();
            let per_logical = buffer_width as f32 / width.max(1.0);
            let zoom = self.zoom.get();
            let cell = zoom as f32 / per_logical;
            let (cx, cy) = (x as f32, y as f32);
            let origin_x = cx - (column as f32 + 0.5) * cell;
            let origin_y = cy - (row as f32 + 0.5) * cell;

            let radius = self.radius.get();
            let circle = graphene::Rect::new(cx - radius, cy - radius, radius * 2.0, radius * 2.0);
            let rounded = gsk::RoundedRect::from_rect(circle, radius);
            snapshot.push_rounded_clip(&rounded);
            snapshot.append_scaled_texture(
                &texture,
                gsk::ScalingFilter::Nearest,
                &graphene::Rect::new(
                    origin_x,
                    origin_y,
                    buffer_width as f32 * cell,
                    buffer_height as f32 * cell,
                ),
            );
            if zoom >= GRID_FROM_ZOOM {
                grid(snapshot, &circle, origin_x, origin_y, cell);
            }
            crosshair(snapshot, cx, cy, cell);
            snapshot.pop();

            let ring = widget.color();
            snapshot.append_border(&rounded, &[RING; 4], &[ring, ring, ring, ring]);
        }
    }

    fn grid(snapshot: &gtk4::Snapshot, circle: &graphene::Rect, x0: f32, y0: f32, cell: f32) {
        let line = gdk::RGBA::new(0.0, 0.0, 0.0, 0.3);
        let first = ((circle.x() - x0) / cell).floor();
        let last = ((circle.x() + circle.width() - x0) / cell).ceil();
        let mut index = first;
        while index <= last {
            let x = x0 + index * cell;
            snapshot.append_color(
                &line,
                &graphene::Rect::new(x, circle.y(), 1.0, circle.height()),
            );
            index += 1.0;
        }
        let first = ((circle.y() - y0) / cell).floor();
        let last = ((circle.y() + circle.height() - y0) / cell).ceil();
        let mut index = first;
        while index <= last {
            let y = y0 + index * cell;
            snapshot.append_color(
                &line,
                &graphene::Rect::new(circle.x(), y, circle.width(), 1.0),
            );
            index += 1.0;
        }
    }

    fn crosshair(snapshot: &gtk4::Snapshot, cx: f32, cy: f32, cell: f32) {
        let side = cell.max(CROSSHAIR_MIN);
        let black = gdk::RGBA::BLACK;
        let white = gdk::RGBA::WHITE;
        for (inset, color) in [(-1.0, black), (0.0, white), (1.0, black)] {
            let edge = side + 2.0 * (1.0 - inset);
            let bounds = graphene::Rect::new(cx - edge / 2.0, cy - edge / 2.0, edge, edge);
            snapshot.append_border(
                &gsk::RoundedRect::from_rect(bounds, 0.0),
                &[1.0; 4],
                &[color; 4],
            );
        }
    }
}

glib::wrapper! {
    pub struct Lens(ObjectSubclass<imp::Lens>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Lens {
    fn new(frame: &Frame, zoom: u32, radius: f32) -> Self {
        let lens: Self = glib::Object::new();
        let texture = gdk::MemoryTexture::new(
            frame.width as i32,
            frame.height as i32,
            gdk::MemoryFormat::R8g8b8x8,
            &frame.pixels,
            frame.width as usize * 4,
        );
        let imp = lens.imp();
        imp.texture.replace(Some(texture.upcast()));
        imp.buffer.set((frame.width, frame.height));
        imp.zoom.set(zoom);
        imp.radius.set(radius);
        lens.set_hexpand(true);
        lens.set_vexpand(true);
        lens
    }

    fn under_pointer(&self) -> (u32, u32) {
        let imp = self.imp();
        let (x, y) = imp.pointer.get().unwrap_or_default();
        pixel_at(
            x,
            y,
            (f64::from(self.width()), f64::from(self.height())),
            imp.buffer.get(),
        )
    }

    fn pixel(&self) -> (u32, u32) {
        let imp = self.imp();
        let buffer = imp.buffer.get();
        let (column, row) = self.under_pointer();
        let (dx, dy) = imp.nudge.get();
        let clamp = |value: u32, delta: i32, size: u32| {
            value
                .saturating_add_signed(delta)
                .min(size.saturating_sub(1))
        };
        (clamp(column, dx, buffer.0), clamp(row, dy, buffer.1))
    }

    fn point(&self, x: f64, y: f64) {
        self.imp().pointer.set(Some((x, y)));
        self.imp().nudge.set((0, 0));
        self.queue_draw();
    }

    fn leave(&self) {
        self.imp().pointer.set(None);
        self.queue_draw();
    }

    fn nudge(&self, dx: i32, dy: i32) {
        let imp = self.imp();
        let (width, height) = imp.buffer.get();
        let (column, row) = self.under_pointer();
        let (nx, ny) = imp.nudge.get();
        let reach = |base: u32, delta: i32, size: u32| {
            let wanted = i64::from(base) + i64::from(delta);
            (wanted.clamp(0, i64::from(size.saturating_sub(1))) - i64::from(base)) as i32
        };
        imp.nudge
            .set((reach(column, nx + dx, width), reach(row, ny + dy, height)));
        self.queue_draw();
    }

    fn set_zoom(&self, zoom: u32) {
        if self.imp().zoom.replace(zoom) != zoom {
            self.queue_draw();
        }
    }

    fn hovered(&self) -> bool {
        self.imp().pointer.get().is_some()
    }
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

    let lens = Lens::new(&frame, zoom, radius);
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
    let Some((x, y)) = surface.lens.imp().pointer.get() else {
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
    fn a_pointer_maps_to_the_buffer_pixel_under_it_at_a_fractional_scale() {
        let logical = (2304.0, 1440.0);
        let buffer = (2880, 1800);

        assert_eq!(pixel_at(0.0, 0.0, logical, buffer), (0, 0));
        assert_eq!(pixel_at(100.0, 100.0, logical, buffer), (125, 125));
        assert_eq!(pixel_at(100.7, 0.5, logical, buffer), (125, 0));
        assert_eq!(pixel_at(100.9, 0.9, logical, buffer), (126, 1));
    }

    #[test]
    fn a_pointer_on_or_past_the_far_edge_reads_the_last_pixel() {
        let logical = (2304.0, 1440.0);
        let buffer = (2880, 1800);

        assert_eq!(pixel_at(2304.0, 1440.0, logical, buffer), (2879, 1799));
        assert_eq!(pixel_at(-4.0, 99999.0, logical, buffer), (0, 1799));
    }

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

    #[test]
    fn scrolling_zooms_in_steps_and_stays_between_one_and_the_maximum() {
        assert_eq!(zoomed(8, -1.0, 30), 10);
        assert_eq!(zoomed(8, 1.0, 30), 6);
        assert_eq!(zoomed(1, -1.0, 30), 2);
        assert_eq!(zoomed(2, 1.0, 30), 1);
        assert_eq!(zoomed(28, -1.0, 30), 30);
        assert_eq!(zoomed(30, -1.0, 30), 30);
        assert_eq!(zoomed(1, 1.0, 30), 1);
        assert_eq!(zoomed(4, 0.0, 30), 4);
        assert_eq!(zoomed(8, -1.0, 5), 5);
    }

    #[test]
    fn the_whole_range_is_a_dozen_notches_either_way() {
        let mut zoom = 1;
        let mut notches = 0;
        while zoom < 30 {
            zoom = zoomed(zoom, -1.0, 30);
            notches += 1;
        }
        assert!(notches <= 16, "took {notches} notches");
    }
}
