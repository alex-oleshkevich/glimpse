use std::cell::RefCell;

use gtk4::{
    CompositeTemplate, TemplateChild, gdk, glib, graphene, prelude::*, subclass::prelude::*,
};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/me/aresa/GlimpseShell/widgets/shot_region.ui")]
pub struct ShotRegion {
    #[template_child]
    pub size_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub hint: TemplateChild<gtk4::Label>,
    #[template_child]
    pub dim_probe: TemplateChild<gtk4::Box>,
    #[template_child]
    pub ring_probe: TemplateChild<gtk4::Box>,

    pub selection: RefCell<Option<(f64, f64, f64, f64)>>,
    pub size_text: RefCell<String>,
}

#[glib::object_subclass]
impl ObjectSubclass for ShotRegion {
    const NAME: &'static str = "ShotRegion";
    type Type = super::ShotRegion;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_accessible_role(gtk4::AccessibleRole::Presentation);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ShotRegion {
    fn constructed(&self) {
        self.parent_constructed();
        self.obj().set_overflow(gtk4::Overflow::Hidden);
    }

    fn dispose(&self) {
        self.dispose_template();
        while let Some(child) = self.obj().first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for ShotRegion {
    fn measure(&self, _orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
        (0, 0, -1, -1)
    }

    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        self.parent_size_allocate(width, height, baseline);
        self.dim_probe.allocate(0, 0, -1, None);
        self.ring_probe.allocate(0, 0, -1, None);
        place_label(&self.size_label, *self.selection.borrow(), width, height);
        place_hint(&self.hint, width);
    }

    fn snapshot(&self, snapshot: &gtk4::Snapshot) {
        let widget = self.obj();
        let width = widget.width() as f32;
        let height = widget.height() as f32;
        paint(
            snapshot,
            width,
            height,
            *self.selection.borrow(),
            &self.dim_probe.color(),
            &widget.color(),
            &self.ring_probe.color(),
        );
        widget.snapshot_child(&self.hint.get(), snapshot);
        if self.size_label.get_visible() {
            widget.snapshot_child(&self.size_label.get(), snapshot);
        }
    }
}

fn place_label(
    label: &gtk4::Label,
    selection: Option<(f64, f64, f64, f64)>,
    width: i32,
    height: i32,
) {
    let Some((x, y, w, h)) = selection.filter(|_| label.get_visible()) else {
        label.allocate(0, 0, -1, None);
        return;
    };
    let (x, y, w, h) = normalize(x, y, w, h);
    let label_width = label.measure(gtk4::Orientation::Horizontal, -1).1.max(1);
    let label_height = label
        .measure(gtk4::Orientation::Vertical, label_width)
        .1
        .max(1);
    let gap = 4;
    let max_x = (width - label_width).max(0);
    let max_y = (height - label_height).max(0);
    let place_x = ((x + w) as i32 - label_width).clamp(0, max_x);
    let below = (y + h) as i32 + gap;
    let place_y = if below + label_height <= height {
        below
    } else {
        ((y + h) as i32 - label_height - gap).clamp(0, max_y)
    };
    label.allocate(
        label_width,
        label_height,
        -1,
        Some(
            gtk4::gsk::Transform::new()
                .translate(&graphene::Point::new(place_x as f32, place_y as f32)),
        ),
    );
}

fn place_hint(label: &gtk4::Label, width: i32) {
    let label_width = label.measure(gtk4::Orientation::Horizontal, -1).1.max(1);
    let label_height = label
        .measure(gtk4::Orientation::Vertical, label_width)
        .1
        .max(1);
    let max_x = (width - label_width).max(0);
    let place_x = ((width - label_width) / 2).clamp(0, max_x);
    label.allocate(
        label_width,
        label_height,
        -1,
        Some(gtk4::gsk::Transform::new().translate(&graphene::Point::new(
            place_x as f32,
            label.margin_top() as f32,
        ))),
    );
}

fn paint(
    snapshot: &gtk4::Snapshot,
    width: f32,
    height: f32,
    selection: Option<(f64, f64, f64, f64)>,
    dim: &gdk::RGBA,
    stroke: &gdk::RGBA,
    ring: &gdk::RGBA,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let Some((x, y, w, h)) = selection.map(|(x, y, w, h)| normalize(x, y, w, h)) else {
        fill(snapshot, dim, 0.0, 0.0, width, height);
        return;
    };
    let right = (x + w).clamp(0.0, width);
    let bottom = (y + h).clamp(0.0, height);
    let left = x.clamp(0.0, width);
    let top = y.clamp(0.0, height);
    let hole_w = right - left;
    let hole_h = bottom - top;
    if hole_w <= 0.0 || hole_h <= 0.0 {
        fill(snapshot, dim, 0.0, 0.0, width, height);
        return;
    }
    fill(snapshot, dim, 0.0, 0.0, width, top);
    fill(snapshot, dim, 0.0, top, left, hole_h);
    fill(snapshot, dim, right, top, width - right, hole_h);
    fill(snapshot, dim, 0.0, bottom, width, height - bottom);
    let ring_bounds = graphene::Rect::new(left - 1.0, top - 1.0, hole_w + 2.0, hole_h + 2.0);
    let stroke_bounds = graphene::Rect::new(left, top, hole_w, hole_h);
    snapshot.append_border(
        &gtk4::gsk::RoundedRect::from_rect(ring_bounds, 0.0),
        &[1.0; 4],
        &[*ring; 4],
    );
    snapshot.append_border(
        &gtk4::gsk::RoundedRect::from_rect(stroke_bounds, 0.0),
        &[1.0; 4],
        &[*stroke; 4],
    );
    for (px, py) in [
        (left, top),
        (left + hole_w / 2.0, top),
        (right, top),
        (left, top + hole_h / 2.0),
        (right, top + hole_h / 2.0),
        (left, bottom),
        (left + hole_w / 2.0, bottom),
        (right, bottom),
    ] {
        handle(snapshot, stroke, ring, px, py);
    }
}

fn handle(snapshot: &gtk4::Snapshot, fill_color: &gdk::RGBA, ring: &gdk::RGBA, cx: f32, cy: f32) {
    let size = 10.0;
    let radius = 3.0;
    let outer = size + 2.0;
    round_fill(
        snapshot,
        ring,
        cx - outer / 2.0,
        cy - outer / 2.0,
        outer,
        radius + 1.0,
    );
    round_fill(
        snapshot,
        fill_color,
        cx - size / 2.0,
        cy - size / 2.0,
        size,
        radius,
    );
}

fn round_fill(
    snapshot: &gtk4::Snapshot,
    color: &gdk::RGBA,
    x: f32,
    y: f32,
    size: f32,
    radius: f32,
) {
    let bounds = graphene::Rect::new(x, y, size, size);
    let rounded = gtk4::gsk::RoundedRect::from_rect(bounds, radius);
    snapshot.push_rounded_clip(&rounded);
    snapshot.append_color(color, &bounds);
    snapshot.pop();
}

fn fill(snapshot: &gtk4::Snapshot, color: &gdk::RGBA, x: f32, y: f32, width: f32, height: f32) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let bounds = graphene::Rect::new(x, y, width, height);
    snapshot.append_color(color, &bounds);
}

fn normalize(x: f64, y: f64, w: f64, h: f64) -> (f32, f32, f32, f32) {
    let (x, w) = if w < 0.0 { (x + w, -w) } else { (x, w) };
    let (y, h) = if h < 0.0 { (y + h, -h) } else { (y, h) };
    (x as f32, y as f32, w as f32, h as f32)
}
