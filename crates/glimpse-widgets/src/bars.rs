use gtk4::{glib, graphene, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};

pub const MAX_VALUES: usize = 366;
const LABEL_MAX_CHARS: usize = 120;
const TOOLTIP_MAX_CHARS: usize = 128;
const MIN_WIDTH: i32 = 8;
const NATURAL_WIDTH: i32 = 160;
const MIN_HEIGHT: i32 = 16;
const NATURAL_HEIGHT: i32 = 40;
const MAX_GAP: i32 = 2;
const MAX_RADIUS: f32 = 2.0;
const MIN_BAR_HEIGHT: f32 = 2.0;
const DIM_ALPHA: f32 = 0.55;
const ZERO_ALPHA: f32 = 0.45;
const EMPTY_ALPHA: f32 = 0.22;

fn clean(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite() && *v >= 0.0)
}

fn layout(width: i32, count: usize, gap: i32) -> Vec<(i32, i32)> {
    if width <= 0 || count == 0 {
        return Vec::new();
    }
    let bars = count.min(width as usize);
    let n = bars as i64;
    let width = width as i64;
    let gap = if bars > 1 {
        (gap.max(0) as i64).min((width - n) / (n - 1))
    } else {
        0
    };
    let room = width - gap * (n - 1);
    let base = room / n;
    let extra = room % n;
    let mut x = 0;
    (0..n)
        .map(|i| {
            let w = base + ((i + 1) * extra / n - i * extra / n);
            let at = x;
            x += w + gap;
            (at as i32, w as i32)
        })
        .collect()
}

fn gap_for(width: i32, count: usize) -> i32 {
    match width.max(0) as usize / count.max(1) {
        0..=2 => 0,
        3..=5 => 1,
        _ => MAX_GAP,
    }
}

fn span(count: usize, bars: usize, i: usize) -> std::ops::Range<usize> {
    i * count / bars..(i + 1) * count / bars
}

fn bucket(values: &[Option<f64>], bars: usize) -> Vec<Option<f64>> {
    (0..bars)
        .map(|i| {
            values[span(values.len(), bars, i)]
                .iter()
                .flatten()
                .copied()
                .reduce(f64::max)
        })
        .collect()
}

fn slot_at(slots: &[(i32, i32)], width: i32, x: i32) -> Option<usize> {
    if x < 0 || x >= width {
        return None;
    }
    slots
        .iter()
        .position(|(at, w)| x < at + w)
        .map(|i| match i {
            0 => 0,
            _ if x >= slots[i].0 => i,
            _ => {
                let back = x - (slots[i - 1].0 + slots[i - 1].1);
                let forward = slots[i].0 - x;
                if back < forward { i - 1 } else { i }
            }
        })
}

fn index_at(width: i32, values: &[Option<f64>], x: i32) -> Option<usize> {
    let slots = layout(width, values.len(), gap_for(width, values.len()));
    let slot = slot_at(&slots, width, x)?;
    let range = span(values.len(), slots.len(), slot);
    let best = range
        .clone()
        .filter_map(|i| values[i].map(|v| (i, v)))
        .fold(None, |best: Option<(usize, f64)>, (i, v)| match best {
            Some((_, top)) if top >= v => best,
            _ => Some((i, v)),
        });
    Some(best.map_or(range.end - 1, |(i, _)| i))
}

fn bar_height(value: f64, maximum: f64, height: f32) -> f32 {
    if value <= 0.0 {
        return 1.0;
    }
    let share = if maximum > 0.0 {
        (value / maximum).min(1.0) as f32
    } else {
        1.0
    };
    (share * height).max(MIN_BAR_HEIGHT).min(height)
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Bars {
        pub values: RefCell<Vec<Option<f64>>>,
        pub maximum: Cell<Option<f64>>,
        pub highlight_last: Cell<bool>,
        pub label: RefCell<String>,
        pub tooltips: RefCell<Vec<Option<String>>>,
        pub hovered: Cell<Option<usize>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Bars {
        const NAME: &'static str = "Bars";
        type Type = super::Bars;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_accessible_role(gtk4::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for Bars {
        fn constructed(&self) {
            self.parent_constructed();
            self.highlight_last.set(true);
            let obj = self.obj();
            obj.add_css_class("bars");
            obj.set_has_tooltip(true);
            obj.connect_query_tooltip(|bars, x, _y, _keyboard, tooltip| {
                match bars.tooltip_target(x) {
                    Some((text, area)) => {
                        tooltip.set_text(Some(&text));
                        tooltip.set_tip_area(&area);
                        true
                    }
                    None => false,
                }
            });
            let motion = gtk4::EventControllerMotion::new();
            motion.connect_motion(glib::clone!(
                #[weak(rename_to = bars)]
                obj,
                move |_, x, _| bars.hover(Some(x as i32))
            ));
            motion.connect_leave(glib::clone!(
                #[weak(rename_to = bars)]
                obj,
                move |_| bars.hover(None)
            ));
            obj.add_controller(motion);
        }
    }

    impl WidgetImpl for Bars {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk4::Orientation::Horizontal => (MIN_WIDTH, NATURAL_WIDTH, -1, -1),
                _ => (MIN_HEIGHT, NATURAL_HEIGHT, -1, -1),
            }
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let obj = self.obj();
            let (width, height) = (obj.width(), obj.height());
            let values = self.values.borrow();
            if width <= 0 || height <= 0 || values.is_empty() {
                return;
            }

            let slots = layout(width, values.len(), gap_for(width, values.len()));
            let shown = bucket(&values, slots.len());
            let maximum = self
                .maximum
                .get()
                .unwrap_or_else(|| shown.iter().flatten().copied().fold(0.0, f64::max));

            let color = obj.color();
            let height = height as f32;
            let last = shown.len() - 1;
            for (i, ((x, w), value)) in slots.iter().zip(&shown).enumerate() {
                let (x, w) = (*x as f32, *w as f32);
                let mut paint = color;
                let h = match value {
                    None => {
                        paint.set_alpha(color.alpha() * EMPTY_ALPHA);
                        1.0
                    }
                    Some(v) => {
                        let dim = self.highlight_last.get() && i != last;
                        let alpha = match (*v <= 0.0, dim) {
                            (true, _) => ZERO_ALPHA,
                            (false, true) => DIM_ALPHA,
                            (false, false) => 1.0,
                        };
                        paint.set_alpha(color.alpha() * alpha);
                        bar_height(*v, maximum, height)
                    }
                };
                let bounds = graphene::Rect::new(x, height - h, w, h);
                let radius = (w / 2.0).min(MAX_RADIUS).min(h);
                if radius < 0.5 {
                    snapshot.append_color(&paint, &bounds);
                    continue;
                }
                let corner = graphene::Size::new(radius, radius);
                let none = graphene::Size::new(0.0, 0.0);
                snapshot.push_rounded_clip(&gtk4::gsk::RoundedRect::new(
                    bounds, corner, corner, none, none,
                ));
                snapshot.append_color(&paint, &bounds);
                snapshot.pop();
            }
        }
    }
}

glib::wrapper! {
    pub struct Bars(ObjectSubclass<imp::Bars>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Bars {
    fn default() -> Self {
        Self::new()
    }
}

impl Bars {
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
        self.refresh_tooltip();
    }

    pub fn set_maximum(&self, maximum: Option<f64>) {
        let maximum = maximum.filter(|m| m.is_finite() && *m > 0.0);
        let imp = self.imp();
        if imp.maximum.get() == maximum {
            return;
        }
        imp.maximum.set(maximum);
        self.queue_draw();
    }

    pub fn set_highlight_last(&self, highlight: bool) {
        let imp = self.imp();
        if imp.highlight_last.get() == highlight {
            return;
        }
        imp.highlight_last.set(highlight);
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

    pub fn set_tooltips(&self, tooltips: &[Option<String>]) {
        let tooltips = &tooltips[tooltips.len().saturating_sub(MAX_VALUES)..];
        let capped: Vec<Option<String>> = tooltips
            .iter()
            .map(|t| {
                t.as_ref()
                    .map(|t| t.chars().take(TOOLTIP_MAX_CHARS).collect())
            })
            .collect();
        let imp = self.imp();
        if *imp.tooltips.borrow() != capped {
            *imp.tooltips.borrow_mut() = capped;
            self.refresh_tooltip();
        }
    }

    fn refresh_tooltip(&self) {
        if self.imp().hovered.get().is_some() {
            self.trigger_tooltip_query();
        }
    }

    pub(crate) fn tooltip_target(&self, x: i32) -> Option<(String, gtk4::gdk::Rectangle)> {
        let imp = self.imp();
        let values = imp.values.borrow();
        let width = self.width();
        let slots = layout(width, values.len(), gap_for(width, values.len()));
        let slot = slot_at(&slots, width, x)?;
        let index = index_at(width, &values, x)?;
        let text = imp.tooltips.borrow().get(index)?.clone()?;
        let (at, w) = slots[slot];
        Some((text, gtk4::gdk::Rectangle::new(at, 0, w, self.height())))
    }

    fn hover(&self, x: Option<i32>) {
        let imp = self.imp();
        let slot = x.and_then(|x| {
            let width = self.width();
            let count = imp.values.borrow().len();
            slot_at(&layout(width, count, gap_for(width, count)), width, x)
        });
        if imp.hovered.get() == slot {
            return;
        }
        imp.hovered.set(slot);
        self.trigger_tooltip_query();
    }

    pub fn values(&self) -> Vec<Option<f64>> {
        self.imp().values.borrow().clone()
    }

    pub fn maximum(&self) -> Option<f64> {
        self.imp().maximum.get()
    }

    pub fn highlight_last(&self) -> bool {
        self.imp().highlight_last.get()
    }

    pub fn label(&self) -> String {
        self.imp().label.borrow().clone()
    }
}

#[cfg(test)]
mod tests;
