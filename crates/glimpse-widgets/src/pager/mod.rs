mod imp;
mod item;

pub use item::PagerItem;

use gtk4::{glib, graphene, prelude::*, subclass::prelude::*};

const DOTS: &str = "pager--dots";
const LABELS: &str = "pager--labels";
const VERTICAL: &str = "pager--vertical";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Shape {
    #[default]
    Dots,
    Labels,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    None,
    Elsewhere,
    Here,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Slot {
    pub id: u64,
    pub label: String,
    pub tooltip: String,
    pub focus: Focus,
    pub occupied: bool,
    pub urgent: bool,
}

glib::wrapper! {
    pub struct Pager(ObjectSubclass<imp::Pager>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for Pager {
    fn default() -> Self {
        Self::new()
    }
}

impl Pager {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_slots(&self, slots: &[Slot]) {
        let imp = self.imp();
        if imp.slots.borrow().as_slice() == slots {
            return;
        }
        imp.slots.replace(slots.to_vec());
        self.render();
    }

    pub fn set_orientation(&self, orientation: gtk4::Orientation) {
        if let Some(layout) = self.layout_manager().and_downcast::<gtk4::BoxLayout>() {
            layout.set_orientation(orientation);
        }

        let vertical = orientation == gtk4::Orientation::Vertical;
        crate::set_css_class(self, VERTICAL, vertical);
        match vertical {
            true => {
                self.set_halign(gtk4::Align::Center);
                self.set_valign(gtk4::Align::Fill);
            }
            false => {
                self.set_halign(gtk4::Align::Fill);
                self.set_valign(gtk4::Align::Center);
            }
        }
    }

    pub fn set_shape(&self, shape: Shape) {
        if self.imp().shape.replace(shape) == shape {
            return;
        }
        self.apply_shape();
        self.render();
    }

    pub fn anchor(&self) -> Option<gtk4::Widget> {
        self.imp().pressed.upgrade().map(Cast::upcast)
    }

    pub(crate) fn item_at(&self, x: f64, y: f64) -> Option<PagerItem> {
        let point = graphene::Point::new(x as f32, y as f32);
        self.imp()
            .items
            .borrow()
            .iter()
            .find(|item| {
                item.compute_bounds(self)
                    .is_some_and(|bounds| bounds.contains_point(&point))
            })
            .cloned()
    }

    pub fn connect_pressed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "pressed",
            false,
            glib::closure_local!(move |pager: Self| f(&pager)),
        )
    }

    pub fn connect_stepped<F: Fn(&Self, bool, bool) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "stepped",
            false,
            glib::closure_local!(move |pager: Self, horizontal: bool, forward: bool| f(
                &pager, horizontal, forward
            )),
        )
    }

    fn apply_shape(&self) {
        let (add, remove) = match self.imp().shape.get() {
            Shape::Labels => (LABELS, DOTS),
            Shape::Dots => (DOTS, LABELS),
        };
        self.remove_css_class(remove);
        self.add_css_class(add);
    }

    fn render(&self) {
        let imp = self.imp();
        let slots = imp.slots.borrow();
        let shape = imp.shape.get();

        let mut items = imp.items.borrow_mut();
        for (index, slot) in slots.iter().enumerate() {
            if items.len() == index {
                let item = PagerItem::new();
                item.insert_after(self, items.last());
                items.push(item);
            }
            items[index].set_slot(slot, shape);
        }

        for item in items.split_off(slots.len()) {
            item.unparent();
        }

        let any = !slots.is_empty();
        drop(items);
        drop(slots);
        self.set_visible(any);
    }
}

fn step(dx: f64, dy: f64) -> Option<(bool, bool)> {
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    match dx.abs() > dy.abs() {
        true => Some((true, dx > 0.0)),
        false => Some((false, dy > 0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::step;

    #[test]
    fn a_scroll_that_went_nowhere_is_not_a_step() {
        assert_eq!(step(0.0, 0.0), None);
    }

    #[test]
    fn the_larger_delta_picks_the_axis() {
        assert_eq!(step(1.0, 0.2), Some((true, true)));
        assert_eq!(step(-1.0, 0.2), Some((true, false)));
        assert_eq!(step(0.2, 1.0), Some((false, true)));
        assert_eq!(step(0.2, -1.0), Some((false, false)));
    }

    #[test]
    fn a_diagonal_of_equal_deltas_resolves_to_the_vertical_axis() {
        assert_eq!(step(1.0, 1.0), Some((false, true)));
        assert_eq!(step(-1.0, 1.0), Some((false, true)));
    }

    #[test]
    fn one_axis_alone_never_reads_as_the_other() {
        assert_eq!(step(3.0, 0.0), Some((true, true)));
        assert_eq!(step(0.0, 3.0), Some((false, true)));
    }
}
