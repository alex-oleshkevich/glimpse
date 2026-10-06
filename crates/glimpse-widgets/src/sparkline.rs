use gtk4::{glib, graphene, gsk, pango, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};

pub const MAX_VALUES: usize = 366;
const LABEL_MAX_CHARS: usize = 120;
const TOOLTIP_MAX_CHARS: usize = 128;
const MIN_WIDTH: i32 = 8;
const NATURAL_WIDTH: i32 = 120;
const MIN_HEIGHT: i32 = 12;
const NATURAL_HEIGHT: i32 = 28;
const STROKE: f32 = 1.5;
const DOT_RADIUS: f32 = 2.5;
const SINGLE_RADIUS: f32 = 1.5;
const EDGE: f32 = DOT_RADIUS;
const VERTICAL_PADDING: f32 = 0.1;
const FILL_ALPHA: f32 = 0.16;
const GRID_ALPHA: f32 = 0.22;

#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub at: f64,
    pub label: String,
}

type Run = Vec<(f32, f32)>;

fn clean(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite())
}

fn plot_edge(width: f32) -> f32 {
    if width > 2.0 * EDGE { EDGE } else { 0.0 }
}

fn point_x(width: f32, count: usize, index: usize) -> f32 {
    let edge = plot_edge(width);
    match count {
        1 => width / 2.0,
        n => edge + (width - 2.0 * edge).max(0.0) * index as f32 / (n - 1) as f32,
    }
}

fn tick_label_x(width: f32, at: f64, label_width: f32) -> f32 {
    let edge = plot_edge(width);
    (edge + (width - 2.0 * edge) * at as f32 - label_width / 2.0)
        .clamp(edge, (width - edge - label_width).max(edge))
}

fn place_labels(width: f32, labels: &[(f64, f32)]) -> Vec<Option<f32>> {
    const GAP: f32 = 6.0;
    let mut placed: Vec<(f32, f32)> = Vec::new();
    let mut out = vec![None; labels.len()];
    for (index, &(at, label_width)) in labels.iter().enumerate().rev() {
        let x = tick_label_x(width, at, label_width);
        let clear = placed
            .iter()
            .all(|&(left, right)| x + label_width + GAP <= left || x >= right + GAP);
        if clear {
            placed.push((x, x + label_width));
            out[index] = Some(x);
        }
    }
    out
}

fn value_y(value: f64, low: f64, high: f64, height: f32, padding: f32) -> f32 {
    let (half_low, span) = (low / 2.0, high / 2.0 - low / 2.0);
    if span > 0.0 {
        let share = ((value / 2.0 - half_low) / span).clamp(0.0, 1.0) as f32;
        height * (1.0 - padding - share * (1.0 - 2.0 * padding))
    } else {
        height / 2.0
    }
}

fn index_at(width: i32, count: usize, x: i32) -> Option<usize> {
    if width <= 0 || count == 0 || x < 0 || x >= width {
        return None;
    }
    let edge = plot_edge(width as f32);
    let share = ((x as f32 - edge) / (width as f32 - 2.0 * edge)).clamp(0.0, 1.0);
    Some((share * (count - 1) as f32).round() as usize)
}

fn measured_height(ticks: &[Tick], caption_height: i32) -> (i32, i32, i32, i32) {
    let extra = if ticks.is_empty() { 0 } else { caption_height };
    (MIN_HEIGHT + extra, NATURAL_HEIGHT + extra, -1, -1)
}

fn runs(
    values: &[Option<f64>],
    width: f32,
    height: f32,
    domain: Option<(f64, f64)>,
    marked: &[bool],
) -> Vec<(bool, Run)> {
    let finite = || values.iter().flatten().copied();
    let Some(low) = finite().reduce(f64::min) else {
        return Vec::new();
    };
    let (low, high) = domain.unwrap_or_else(|| (low, finite().fold(low, f64::max)));
    let padding = if domain.is_some() {
        (STROKE / (2.0 * height)).min(0.5)
    } else {
        VERTICAL_PADDING
    };

    let mut out: Vec<(bool, Run)> = Vec::new();
    let mut open = false;
    for (i, value) in values.iter().enumerate() {
        match value {
            Some(v) => {
                let mark = marked.get(i).copied().unwrap_or(false);
                let point = (
                    point_x(width, values.len(), i),
                    value_y(*v, low, high, height, padding),
                );
                if !open {
                    out.push((mark, vec![point]));
                    open = true;
                    continue;
                }
                let Some((previous_mark, run)) = out.last_mut() else {
                    return out;
                };
                if *previous_mark == mark {
                    run.push(point);
                } else {
                    let previous = run[run.len() - 1];
                    let middle = ((previous.0 + point.0) / 2.0, (previous.1 + point.1) / 2.0);
                    run.push(middle);
                    out.push((mark, vec![middle, point]));
                }
            }
            None => open = false,
        }
    }
    out
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Sparkline {
        pub values: RefCell<Vec<Option<f64>>>,
        pub domain: Cell<Option<(f64, f64)>>,
        pub ticks: RefCell<Vec<Tick>>,
        pub marked: RefCell<Vec<bool>>,
        pub tooltips: RefCell<Vec<Option<String>>>,
        pub hovered: Cell<Option<usize>>,
        pub mark_style: gtk4::Label,
        pub tick_style: gtk4::Label,
        pub fill: Cell<bool>,
        pub show_last: Cell<bool>,
        pub label: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Sparkline {
        const NAME: &'static str = "Sparkline";
        type Type = super::Sparkline;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_accessible_role(gtk4::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for Sparkline {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.add_css_class("sparkline");
            for (child, class) in [
                (&self.mark_style, "sparkline__mark"),
                (&self.tick_style, "sparkline__ticks"),
            ] {
                child.add_css_class(class);
                child.set_can_target(false);
                child.set_parent(&*obj);
                child.set_child_visible(false);
            }
            obj.connect_query_tooltip(|sparkline, x, _y, _keyboard, tooltip| {
                if let Some(text) = sparkline.tooltip_target(x) {
                    tooltip.set_text(Some(&text));
                    true
                } else {
                    false
                }
            });
            let motion = gtk4::EventControllerMotion::new();
            motion.connect_motion(glib::clone!(
                #[weak(rename_to = sparkline)]
                obj,
                move |_, x, _| sparkline.hover(Some(x as i32))
            ));
            motion.connect_leave(glib::clone!(
                #[weak(rename_to = sparkline)]
                obj,
                move |_| sparkline.hover(None)
            ));
            obj.add_controller(motion);
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Sparkline {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk4::Orientation::Horizontal => (MIN_WIDTH, NATURAL_WIDTH, -1, -1),
                _ => measured_height(&self.ticks.borrow(), self.caption_height()),
            }
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let obj = self.obj();
            let width = obj.width() as f32;
            let ticks = self.ticks.borrow();
            let height = (obj.height() - self.caption_height()).max(0) as f32;
            if width <= 0.0 || height <= 0.0 {
                return;
            }
            let runs = runs(
                &self.values.borrow(),
                width,
                height,
                self.domain.get(),
                &self.marked.borrow(),
            );

            let color = obj.color();
            let accent = self.mark_style.color();
            let stroke = gsk::Stroke::builder(STROKE)
                .line_cap(gsk::LineCap::Round)
                .line_join(gsk::LineJoin::Round)
                .build();

            snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, width, height));
            let edge = plot_edge(width);
            let mut grid_color = color;
            grid_color.set_alpha(color.alpha() * GRID_ALPHA);
            for tick in ticks.iter() {
                let x = edge + (width - 2.0 * edge) * tick.at as f32;
                snapshot.append_color(&grid_color, &graphene::Rect::new(x - 0.5, 0.0, 1.0, height));
            }
            for (marked, run) in &runs {
                let paint = if *marked { accent } else { color };
                let mut soft = paint;
                soft.set_alpha(paint.alpha() * FILL_ALPHA);
                let (first, last) = (run[0], run[run.len() - 1]);
                if run.len() == 1 {
                    let dot = gsk::PathBuilder::new();
                    dot.add_circle(&graphene::Point::new(first.0, first.1), SINGLE_RADIUS);
                    snapshot.append_fill(&dot.to_path(), gsk::FillRule::Winding, &paint);
                    continue;
                }
                if self.fill.get() {
                    let area = gsk::PathBuilder::new();
                    area.move_to(first.0, height);
                    for (x, y) in run {
                        area.line_to(*x, *y);
                    }
                    area.line_to(last.0, height);
                    area.close();
                    snapshot.append_fill(&area.to_path(), gsk::FillRule::Winding, &soft);
                }
                let line = gsk::PathBuilder::new();
                line.move_to(first.0, first.1);
                for (x, y) in &run[1..] {
                    line.line_to(*x, *y);
                }
                snapshot.append_stroke(&line.to_path(), &stroke, &paint);
            }
            let ends_with_data = matches!(self.values.borrow().last(), Some(Some(_)));
            if let Some((marked, run)) = runs.last()
                && self.show_last.get()
                && ends_with_data
            {
                let (x, y) = run[run.len() - 1];
                let paint = if *marked { accent } else { color };
                let dot = gsk::PathBuilder::new();
                dot.add_circle(&graphene::Point::new(x, y), DOT_RADIUS);
                snapshot.append_fill(&dot.to_path(), gsk::FillRule::Winding, &paint);
            }
            snapshot.pop();
            if !ticks.is_empty() {
                let layout = self.caption_layout();
                layout.set_width(((width - 2.0 * edge).max(0.0) * pango::SCALE as f32) as i32);
                layout.set_ellipsize(pango::EllipsizeMode::End);
                layout.set_height(-1);
                let widths: Vec<(f64, f32)> = ticks
                    .iter()
                    .map(|tick| {
                        layout.set_text(&tick.label);
                        (tick.at, layout.pixel_size().0 as f32)
                    })
                    .collect();
                for (tick, x) in ticks.iter().zip(place_labels(width, &widths)) {
                    let Some(x) = x else {
                        continue;
                    };
                    layout.set_text(&tick.label);
                    snapshot.save();
                    snapshot.translate(&graphene::Point::new(x, height));
                    snapshot.append_layout(&layout, &self.tick_style.color());
                    snapshot.restore();
                }
            }
        }
    }

    impl Sparkline {
        fn caption_layout(&self) -> pango::Layout {
            let layout = self.obj().create_pango_layout(Some("Mg"));
            layout
                .set_font_description(self.tick_style.pango_context().font_description().as_ref());
            layout.set_single_paragraph_mode(true);
            layout
        }

        fn caption_height(&self) -> i32 {
            if self.ticks.borrow().is_empty() {
                0
            } else {
                self.caption_layout().pixel_size().1
            }
        }
    }
}

glib::wrapper! {
    pub struct Sparkline(ObjectSubclass<imp::Sparkline>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Sparkline {
    fn default() -> Self {
        Self::new()
    }
}

impl Sparkline {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_values(&self, values: &[Option<f64>]) {
        let values = &values[values.len().saturating_sub(MAX_VALUES)..];
        let cleaned: Vec<Option<f64>> = values.iter().map(|v| clean(*v)).collect();
        let imp = self.imp();
        if *imp.values.borrow() == cleaned {
            return;
        }
        *imp.values.borrow_mut() = cleaned;
        self.queue_draw();
    }

    pub fn set_domain(&self, domain: Option<(f64, f64)>) {
        let domain = domain.filter(|(low, high)| low.is_finite() && high.is_finite() && low < high);
        let imp = self.imp();
        if imp.domain.get() == domain {
            return;
        }
        imp.domain.set(domain);
        self.queue_draw();
    }

    pub fn set_ticks(&self, ticks: &[Tick]) {
        let ticks: Vec<Tick> = ticks
            .iter()
            .take(MAX_VALUES)
            .filter(|tick| tick.at.is_finite())
            .map(|tick| Tick {
                at: tick.at.clamp(0.0, 1.0),
                label: tick.label.chars().take(LABEL_MAX_CHARS).collect(),
            })
            .collect();
        let imp = self.imp();
        if *imp.ticks.borrow() == ticks {
            return;
        }
        *imp.ticks.borrow_mut() = ticks;
        self.queue_resize();
        self.queue_draw();
    }

    pub fn set_marked(&self, marked: &[bool]) {
        let marked = &marked[marked.len().saturating_sub(MAX_VALUES)..];
        let imp = self.imp();
        if imp.marked.borrow().as_slice() == marked {
            return;
        }
        *imp.marked.borrow_mut() = marked.to_vec();
        self.queue_draw();
    }

    pub fn set_tooltips(&self, tooltips: &[Option<String>]) {
        let tooltips = &tooltips[tooltips.len().saturating_sub(MAX_VALUES)..];
        let capped: Vec<Option<String>> = tooltips
            .iter()
            .map(|text| {
                text.as_ref()
                    .map(|text| text.chars().take(TOOLTIP_MAX_CHARS).collect())
            })
            .collect();
        let imp = self.imp();
        if *imp.tooltips.borrow() == capped {
            return;
        }
        let has_tooltip = capped.iter().any(Option::is_some);
        *imp.tooltips.borrow_mut() = capped;
        self.set_has_tooltip(has_tooltip);
        self.queue_draw();
        if imp.hovered.get().is_some() {
            self.trigger_tooltip_query();
        }
    }

    fn tooltip_target(&self, x: i32) -> Option<String> {
        let imp = self.imp();
        let values = imp.values.borrow();
        let index = index_at(self.width(), values.len(), x)?;
        values[index]?;
        imp.tooltips.borrow().get(index)?.clone()
    }

    fn hover(&self, x: Option<i32>) {
        let imp = self.imp();
        let index = x.and_then(|x| index_at(self.width(), imp.values.borrow().len(), x));
        if imp.hovered.get() == index {
            return;
        }
        imp.hovered.set(index);
        if self.has_tooltip() {
            self.trigger_tooltip_query();
        }
    }

    pub fn set_fill(&self, fill: bool) {
        let imp = self.imp();
        if imp.fill.get() == fill {
            return;
        }
        imp.fill.set(fill);
        self.queue_draw();
    }

    pub fn set_show_last(&self, show: bool) {
        let imp = self.imp();
        if imp.show_last.get() == show {
            return;
        }
        imp.show_last.set(show);
        self.queue_draw();
    }

    pub fn set_label(&self, label: &str) {
        let label: String = label.chars().take(LABEL_MAX_CHARS).collect();
        let imp = self.imp();
        if *imp.label.borrow() == label {
            return;
        }
        self.update_property(&[gtk4::accessible::Property::Label(&label)]);
        *imp.label.borrow_mut() = label;
    }

    pub fn values(&self) -> Vec<Option<f64>> {
        self.imp().values.borrow().clone()
    }

    pub fn fill(&self) -> bool {
        self.imp().fill.get()
    }

    pub fn show_last(&self) -> bool {
        self.imp().show_last.get()
    }

    pub fn label(&self) -> String {
        self.imp().label.borrow().clone()
    }
}

#[cfg(test)]
mod tests;
