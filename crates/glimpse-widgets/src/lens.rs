use std::cell::{Cell, RefCell};

use gtk4::{gdk, glib, graphene, gsk, prelude::*, subclass::prelude::*};

const ZOOM_STEP: f64 = 1.25;
const RING: f32 = 2.0;
const GRID_FROM_ZOOM: u32 = 6;
const CROSSHAIR_MIN: f32 = 7.0;

fn pixel_at(x: f64, y: f64, logical: (f64, f64), buffer: (u32, u32)) -> (u32, u32) {
    let column = (x / logical.0 * f64::from(buffer.0)).floor();
    let row = (y / logical.1 * f64::from(buffer.1)).floor();
    let clamp = |value: f64, size: u32| value.clamp(0.0, f64::from(size.saturating_sub(1))) as u32;
    (clamp(column, buffer.0), clamp(row, buffer.1))
}

pub fn zoomed(zoom: u32, notches: f64, max: u32) -> u32 {
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
    pub fn new(texture: &gdk::Texture, buffer: (u32, u32), zoom: u32, radius: f32) -> Self {
        let lens: Self = glib::Object::new();
        let imp = lens.imp();
        imp.texture.replace(Some(texture.clone()));
        imp.buffer.set(buffer);
        imp.zoom.set(zoom);
        imp.radius.set(radius);
        lens.set_hexpand(true);
        lens.set_vexpand(true);
        lens
    }

    pub fn under_pointer(&self) -> (u32, u32) {
        let imp = self.imp();
        let (x, y) = imp.pointer.get().unwrap_or_default();
        pixel_at(
            x,
            y,
            (f64::from(self.width()), f64::from(self.height())),
            imp.buffer.get(),
        )
    }

    pub fn pixel(&self) -> (u32, u32) {
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

    pub fn point(&self, x: f64, y: f64) {
        self.imp().pointer.set(Some((x, y)));
        self.imp().nudge.set((0, 0));
        self.queue_draw();
    }

    pub fn leave(&self) {
        self.imp().pointer.set(None);
        self.queue_draw();
    }

    pub fn nudge(&self, dx: i32, dy: i32) {
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

    pub fn set_zoom(&self, zoom: u32) {
        if self.imp().zoom.replace(zoom) != zoom {
            self.queue_draw();
        }
    }

    pub fn set_radius(&self, radius: f32) {
        if self.imp().radius.replace(radius) != radius {
            self.queue_draw();
        }
    }

    pub fn hovered(&self) -> bool {
        self.imp().pointer.get().is_some()
    }

    pub fn pointer(&self) -> Option<(f64, f64)> {
        self.imp().pointer.get()
    }
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
