use gtk4::{glib, graphene, gsk, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};

pub const MAX_VALUES: usize = 366;
const LABEL_MAX_CHARS: usize = 120;
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

type Run = Vec<(f32, f32)>;

fn clean(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite())
}

fn runs(values: &[Option<f64>], width: f32, height: f32) -> Vec<Run> {
    let finite = || values.iter().flatten().copied();
    let Some(low) = finite().reduce(f64::min) else {
        return Vec::new();
    };
    let high = finite().fold(low, f64::max);
    let (half_low, span) = (low / 2.0, high / 2.0 - low / 2.0);
    let edge = if width > 2.0 * EDGE { EDGE } else { 0.0 };
    let run = (width - 2.0 * edge).max(0.0);
    let x = |i: usize| match values.len() {
        1 => width / 2.0,
        n => edge + run * i as f32 / (n - 1) as f32,
    };
    let y = |v: f64| {
        if span > 0.0 {
            let share = ((v / 2.0 - half_low) / span).clamp(0.0, 1.0) as f32;
            height * (1.0 - VERTICAL_PADDING - share * (1.0 - 2.0 * VERTICAL_PADDING))
        } else {
            height / 2.0
        }
    };

    let mut out: Vec<Run> = Vec::new();
    let mut open = false;
    for (i, value) in values.iter().enumerate() {
        match value {
            Some(v) => {
                if !open {
                    out.push(Vec::new());
                    open = true;
                }
                out.last_mut().expect("open run").push((x(i), y(*v)));
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
            self.obj().add_css_class("sparkline");
        }
    }

    impl WidgetImpl for Sparkline {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk4::Orientation::Horizontal => (MIN_WIDTH, NATURAL_WIDTH, -1, -1),
                _ => (MIN_HEIGHT, NATURAL_HEIGHT, -1, -1),
            }
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let obj = self.obj();
            let (width, height) = (obj.width() as f32, obj.height() as f32);
            if width <= 0.0 || height <= 0.0 {
                return;
            }
            let runs = runs(&self.values.borrow(), width, height);
            if runs.is_empty() {
                return;
            }

            let color = obj.color();
            let mut soft = color;
            soft.set_alpha(color.alpha() * FILL_ALPHA);
            let stroke = gsk::Stroke::builder(STROKE)
                .line_cap(gsk::LineCap::Round)
                .line_join(gsk::LineJoin::Round)
                .build();

            snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, width, height));
            for run in &runs {
                let (first, last) = (run[0], run[run.len() - 1]);
                if run.len() == 1 {
                    let dot = gsk::PathBuilder::new();
                    dot.add_circle(&graphene::Point::new(first.0, first.1), SINGLE_RADIUS);
                    snapshot.append_fill(&dot.to_path(), gsk::FillRule::Winding, &color);
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
                snapshot.append_stroke(&line.to_path(), &stroke, &color);
            }
            let ends_with_data = matches!(self.values.borrow().last(), Some(Some(_)));
            if let Some(&(x, y)) = runs.last().and_then(|run| run.last())
                && self.show_last.get()
                && ends_with_data
            {
                let dot = gsk::PathBuilder::new();
                dot.add_circle(&graphene::Point::new(x, y), DOT_RADIUS);
                snapshot.append_fill(&dot.to_path(), gsk::FillRule::Winding, &color);
            }
            snapshot.pop();
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
