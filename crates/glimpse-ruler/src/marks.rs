use std::cell::{Cell, RefCell};

use gtk4::{gdk, glib, graphene, gsk, prelude::*, subclass::prelude::*};

use crate::session::Segment;

const LINE: f32 = 2.0;
const HALO: f32 = 4.0;
const DOT: f32 = 3.5;
const TICK: u32 = 10;
const MAJOR: u32 = 50;
const LABELED: u32 = 100;
const LABEL_GAP: f32 = 3.0;

fn halo() -> gdk::RGBA {
    gdk::RGBA::new(0.0, 0.0, 0.0, 0.45)
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Marks {
        pub buffer: Cell<(u32, u32)>,
        pub radius: Cell<f32>,
        pub segments: RefCell<Vec<Segment>>,
        pub anchor: Cell<Option<(u32, u32)>>,
        pub live: Cell<Option<(u32, u32)>>,
        pub pointer: Cell<Option<(f64, f64)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Marks {
        const NAME: &'static str = "RulerMarks";
        type Type = super::Marks;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk4::BinLayout>();
        }
    }

    impl ObjectImpl for Marks {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Marks {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let (width, height) = (widget.width() as f32, widget.height() as f32);
            let (columns, rows) = self.buffer.get();
            let at = |(x, y): (u32, u32)| {
                graphene::Point::new(
                    (x as f32 + 0.5) * width / columns.max(1) as f32,
                    (y as f32 + 0.5) * height / rows.max(1) as f32,
                )
            };
            let color = widget.color();

            let hole = self.pointer.get();
            if let Some((x, y)) = hole {
                snapshot.push_mask(gsk::MaskMode::InvertedAlpha);
                let circle = gsk::PathBuilder::new();
                circle.add_circle(&graphene::Point::new(x as f32, y as f32), self.radius.get());
                snapshot.append_fill(&circle.to_path(), gsk::FillRule::Winding, &color);
                snapshot.pop();
            }

            let mut child = widget.first_child();
            while let Some(track) = child {
                widget.snapshot_child(&track, snapshot);
                child = track.next_sibling();
            }

            if hole.is_some() {
                snapshot.pop();
            }

            let live = self.live.get();
            if let Some(point) = live.map(at) {
                let hair = color.with_alpha(0.55);
                snapshot.append_color(
                    &hair,
                    &graphene::Rect::new(0.0, point.y().floor(), width, 1.0),
                );
                snapshot.append_color(
                    &hair,
                    &graphene::Rect::new(point.x().floor(), 0.0, 1.0, height),
                );
            }

            let lines = gsk::PathBuilder::new();
            let dots = gsk::PathBuilder::new();
            let mut drawn = false;
            let mut line = |from: (u32, u32), to: (u32, u32)| {
                lines.move_to(at(from).x(), at(from).y());
                lines.line_to(at(to).x(), at(to).y());
                drawn = true;
            };
            for segment in self.segments.borrow().iter() {
                line(segment.from, segment.to);
                dots.add_circle(&at(segment.from), DOT);
                dots.add_circle(&at(segment.to), DOT);
            }
            let anchor = self.anchor.get();
            if let Some(anchor) = anchor {
                if let Some(live) = live {
                    line(anchor, live);
                }
                dots.add_circle(&at(anchor), DOT);
            }
            if drawn {
                let path = lines.to_path();
                let halo_stroke = gsk::Stroke::new(LINE + HALO);
                halo_stroke.set_line_cap(gsk::LineCap::Round);
                snapshot.append_stroke(&path, &halo_stroke, &halo());
                let stroke = gsk::Stroke::new(LINE);
                stroke.set_line_cap(gsk::LineCap::Round);
                snapshot.append_stroke(&path, &stroke, &color);
            }
            let dots = dots.to_path();
            if !dots.is_empty() {
                snapshot.append_stroke(&dots, &gsk::Stroke::new(HALO), &halo());
                snapshot.append_fill(&dots, gsk::FillRule::Winding, &color);
            }
        }
    }

    #[derive(Default)]
    pub struct Track {
        pub vertical: Cell<bool>,
        pub length: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Track {
        const NAME: &'static str = "RulerTrack";
        type Type = super::Track;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Track {}

    impl WidgetImpl for Track {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let vertical = self.vertical.get();
            let (width, height) = (widget.width() as f32, widget.height() as f32);
            let (extent, depth) = if vertical {
                (height, width)
            } else {
                (width, height)
            };
            let length = self.length.get().max(1);
            let per_pixel = extent / length as f32;
            let color = widget.color();
            for pixel in (0..length).step_by(TICK as usize) {
                let along = (pixel as f32 * per_pixel).floor();
                let reach = depth * reach(pixel);
                let tick = if vertical {
                    graphene::Rect::new(depth - reach, along, reach, 1.0)
                } else {
                    graphene::Rect::new(along, depth - reach, 1.0, reach)
                };
                snapshot.append_color(&color, &tick);
                if pixel == 0 || !pixel.is_multiple_of(LABELED) {
                    continue;
                }
                let layout = widget.create_pango_layout(Some(&pixel.to_string()));
                let origin = if vertical {
                    graphene::Point::new(LABEL_GAP, along + LABEL_GAP)
                } else {
                    graphene::Point::new(along + LABEL_GAP, 0.0)
                };
                snapshot.save();
                snapshot.translate(&origin);
                snapshot.append_layout(&layout, &color);
                snapshot.restore();
            }
        }
    }
}

fn reach(pixel: u32) -> f32 {
    if pixel.is_multiple_of(LABELED) {
        1.0
    } else if pixel.is_multiple_of(MAJOR) {
        0.5
    } else {
        0.25
    }
}

glib::wrapper! {
    pub struct Marks(ObjectSubclass<imp::Marks>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Marks {
    pub fn new(buffer: (u32, u32), radius: f32) -> Self {
        let marks: Self = glib::Object::new();
        marks.imp().buffer.set(buffer);
        marks.imp().radius.set(radius);
        marks.add_css_class("ruler__marks");
        marks.set_can_target(false);
        Track::new(false, buffer.0).set_parent(&marks);
        Track::new(true, buffer.1).set_parent(&marks);
        marks
    }

    pub fn set(
        &self,
        segments: Vec<Segment>,
        anchor: Option<(u32, u32)>,
        live: Option<(u32, u32)>,
        pointer: Option<(f64, f64)>,
    ) {
        let imp = self.imp();
        imp.segments.replace(segments);
        imp.anchor.set(anchor);
        imp.live.set(live);
        imp.pointer.set(pointer);
        self.queue_draw();
    }

    pub fn set_radius(&self, radius: f32) {
        if self.imp().radius.replace(radius) != radius {
            self.queue_draw();
        }
    }
}

glib::wrapper! {
    pub struct Track(ObjectSubclass<imp::Track>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Track {
    pub fn new(vertical: bool, length: u32) -> Self {
        let track: Self = glib::Object::new();
        track.imp().vertical.set(vertical);
        track.imp().length.set(length);
        track.add_css_class("ruler__track");
        track.set_can_target(false);
        if vertical {
            track.set_halign(gtk4::Align::Start);
            track.set_valign(gtk4::Align::Fill);
        } else {
            track.set_halign(gtk4::Align::Fill);
            track.set_valign(gtk4::Align::Start);
        }
        track
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hundredth_pixel_reaches_across_the_track_and_every_fiftieth_halfway() {
        assert_eq!(reach(0), 1.0);
        assert_eq!(reach(300), 1.0);
        assert_eq!(reach(250), 0.5);
        assert_eq!(reach(260), 0.25);
    }
}
