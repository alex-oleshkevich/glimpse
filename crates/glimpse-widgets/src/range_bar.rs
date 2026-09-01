use gtk4::{glib, graphene, prelude::*, subclass::prelude::*};
use std::cell::Cell;

const THICKNESS: f32 = 4.0;
const MIN_WIDTH: i32 = 24;
const NATURAL_WIDTH: i32 = 96;
const TRACK_ALPHA: f32 = 0.22;

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct RangeBar {
        pub low: Cell<f64>,
        pub high: Cell<f64>,
        pub minimum: Cell<f64>,
        pub maximum: Cell<f64>,
    }

    impl Default for RangeBar {
        fn default() -> Self {
            Self {
                low: Cell::new(0.0),
                high: Cell::new(0.0),
                minimum: Cell::new(0.0),
                maximum: Cell::new(1.0),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RangeBar {
        const NAME: &'static str = "RangeBar";
        type Type = super::RangeBar;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for RangeBar {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().add_css_class("range-bar");
        }
    }

    impl WidgetImpl for RangeBar {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let thickness = THICKNESS.ceil() as i32;
            match orientation {
                gtk4::Orientation::Horizontal => (MIN_WIDTH, NATURAL_WIDTH, -1, -1),
                _ => (thickness, thickness, -1, -1),
            }
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let obj = self.obj();
            let (width, height) = (obj.width() as f32, obj.height() as f32);
            if width <= 0.0 {
                return;
            }

            let color = obj.color();
            let mut track = color;
            track.set_alpha(color.alpha() * TRACK_ALPHA);
            let y = (height - THICKNESS) / 2.0;

            fill(snapshot, &track, 0.0, width, y);

            let span = self.maximum.get() - self.minimum.get();
            if span <= 0.0 {
                return;
            }
            let at =
                |value: f64| ((value - self.minimum.get()) / span).clamp(0.0, 1.0) as f32 * width;
            let start = at(self.low.get()).min(width - THICKNESS).max(0.0);
            let end = at(self.high.get()).max(start + THICKNESS).min(width);
            fill(snapshot, &color, start, end, y);
        }
    }

    fn fill(snapshot: &gtk4::Snapshot, color: &gtk4::gdk::RGBA, from: f32, to: f32, y: f32) {
        let bounds = graphene::Rect::new(from, y, (to - from).max(THICKNESS), THICKNESS);
        let rounded = gtk4::gsk::RoundedRect::from_rect(bounds, THICKNESS / 2.0);
        snapshot.push_rounded_clip(&rounded);
        snapshot.append_color(color, &bounds);
        snapshot.pop();
    }
}

glib::wrapper! {
    pub struct RangeBar(ObjectSubclass<imp::RangeBar>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for RangeBar {
    fn default() -> Self {
        Self::new()
    }
}

impl RangeBar {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_range(&self, low: f64, high: f64) {
        let high = high.max(low);
        let imp = self.imp();
        if imp.low.get() == low && imp.high.get() == high {
            return;
        }
        imp.low.set(low);
        imp.high.set(high);
        self.queue_draw();
    }

    pub fn set_scale(&self, minimum: f64, maximum: f64) {
        let imp = self.imp();
        if imp.minimum.get() == minimum && imp.maximum.get() == maximum {
            return;
        }
        imp.minimum.set(minimum);
        imp.maximum.set(maximum);
        self.queue_draw();
    }

    pub fn range(&self) -> (f64, f64) {
        (self.imp().low.get(), self.imp().high.get())
    }

    pub fn scale(&self) -> (f64, f64) {
        (self.imp().minimum.get(), self.imp().maximum.get())
    }
}
