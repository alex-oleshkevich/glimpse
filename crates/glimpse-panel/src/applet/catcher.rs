use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::AnimationExt;
use glimpse_config::Position;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const DURATION: u32 = 150;
const SIDEWAYS: &str = "applet-popover__arrow--sideways";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Closed,
    Opening,
    Open,
    Closing,
}

pub struct Catcher {
    window: gtk4::Window,
    slot: gtk4::Box,
    arrow: gtk4::DrawingArea,
    body: gtk4::Box,
    fade: adw::TimedAnimation,
    side: Cell<Position>,
    state: Cell<State>,
    center: Cell<i32>,
    dismissed: RefCell<Option<Box<dyn Fn()>>>,
}

impl Catcher {
    pub fn new(monitor: Option<&gtk4::gdk::Monitor>, side: Position) -> Rc<Self> {
        let window = gtk4::Window::new();
        window.init_layer_shell();
        window.set_namespace(Some("glimpse-popover"));
        window.set_layer(Layer::Top);
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_exclusive_zone(0);
        window.add_css_class("popover-catcher");
        for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            window.set_anchor(edge, true);
        }
        if let Some(monitor) = monitor {
            window.set_monitor(Some(monitor));
        }

        let slot = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        slot.set_opacity(0.0);

        let arrow = gtk4::DrawingArea::new();
        arrow.add_css_class("applet-popover__arrow");
        slot.append(&arrow);

        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        body.add_css_class("applet-popover__body");
        slot.append(&body);

        window.set_child(Some(&slot));

        let fade = adw::TimedAnimation::new(
            &slot,
            0.0,
            1.0,
            DURATION,
            adw::PropertyAnimationTarget::new(&slot, "opacity"),
        );
        fade.set_easing(adw::Easing::EaseOutCubic);

        let catcher = Rc::new(Self {
            window: window.clone(),
            slot,
            arrow,
            body,
            fade,
            side: Cell::new(side),
            state: Cell::default(),
            center: Cell::default(),
            dismissed: RefCell::default(),
        });

        catcher.arrow.set_draw_func({
            let catcher = Rc::downgrade(&catcher);
            move |area, context, width, height| {
                let Some(catcher) = catcher.upgrade() else {
                    return;
                };
                point(catcher.side.get(), area, context, width, height);
            }
        });

        catcher.fade.connect_done({
            let catcher = Rc::downgrade(&catcher);
            move |_| {
                let Some(catcher) = catcher.upgrade() else {
                    return;
                };
                match catcher.state.get() {
                    State::Opening => catcher.state.set(State::Open),
                    State::Closing => catcher.finish_close(),
                    State::Closed | State::Open => (),
                }
            }
        });

        let press = gtk4::GestureClick::new();
        press.connect_pressed({
            let catcher = Rc::downgrade(&catcher);
            let window = window.clone();
            move |_, _, x, y| {
                let Some(catcher) = catcher.upgrade() else {
                    return;
                };
                let under = window.pick(x, y, gtk4::PickFlags::DEFAULT);
                let inside = under.is_some_and(|widget| {
                    widget == catcher.slot || widget.is_ancestor(&catcher.slot)
                });
                if !inside {
                    catcher.close();
                }
            }
        });
        window.add_controller(press);

        catcher.layout(side);

        catcher
    }

    pub fn open(
        self: &Rc<Self>,
        child: &impl IsA<gtk4::Widget>,
        center: i32,
        dismissed: impl Fn() + 'static,
    ) {
        self.release();
        self.fade.reset();
        self.center.set(center);
        self.dismissed.replace(Some(Box::new(dismissed)));
        self.body.append(child.as_ref());
        self.state.set(State::Opening);
        self.slot.set_opacity(0.0);
        self.window.present();

        // A layer surface has no size and is not mapped until the compositor configures it, so
        // an idle right after present() has nothing to centre against and animates nothing.
        let catcher = Rc::downgrade(self);
        self.slot.add_tick_callback(move |slot, _| {
            let Some(catcher) = catcher.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if catcher.state.get() != State::Opening
                || catcher.fade.state() == adw::AnimationState::Playing
            {
                return glib::ControlFlow::Break;
            }
            if slot.width() == 0 {
                return glib::ControlFlow::Continue;
            }
            catcher.settle();
            catcher.fade.set_value_from(0.0);
            catcher.fade.set_value_to(1.0);
            catcher.fade.play();
            glib::ControlFlow::Break
        });
    }

    pub fn close(&self) {
        if matches!(self.state.get(), State::Closed | State::Closing) {
            return;
        }
        let from = self.slot.opacity();
        self.state.set(State::Closing);
        self.fade.reset();
        self.fade.set_value_from(from);
        self.fade.set_value_to(0.0);
        self.fade.play();
    }

    pub fn place(&self, center: i32) {
        self.center.set(center);
        self.settle();
    }

    pub fn holds(&self, widget: &impl IsA<gtk4::Widget>) -> bool {
        self.body.first_child().as_ref() == Some(widget.as_ref())
    }

    pub fn horizontal(&self) -> bool {
        horizontal(self.side.get())
    }

    pub fn reconfigure(&self, monitor: &gtk4::gdk::Monitor, side: Position) {
        if self.window.monitor().as_ref() != Some(monitor) {
            self.window.set_monitor(Some(monitor));
        }
        if side != self.side.get() {
            self.finish_close();
        }
        self.layout(side);
    }

    fn layout(&self, side: Position) {
        self.side.set(side);

        self.slot.set_orientation(match horizontal(side) {
            true => gtk4::Orientation::Vertical,
            false => gtk4::Orientation::Horizontal,
        });
        self.slot.set_halign(match side {
            Position::Right => gtk4::Align::End,
            _ => gtk4::Align::Start,
        });
        self.slot.set_valign(match side {
            Position::Bottom => gtk4::Align::End,
            _ => gtk4::Align::Start,
        });
        self.slot.reorder_child_after(
            &self.arrow,
            match side {
                Position::Top | Position::Left => None,
                Position::Bottom | Position::Right => Some(&self.body),
            },
        );

        match horizontal(side) {
            true => {
                self.arrow.set_halign(gtk4::Align::Start);
                self.arrow.set_valign(gtk4::Align::Fill);
                self.arrow.remove_css_class(SIDEWAYS);
            }
            false => {
                self.arrow.set_halign(gtk4::Align::Fill);
                self.arrow.set_valign(gtk4::Align::Start);
                self.arrow.add_css_class(SIDEWAYS);
            }
        }
        self.arrow.set_margin_start(0);
        self.arrow.set_margin_top(0);
        self.slot.set_margin_start(0);
        self.slot.set_margin_top(0);
        self.arrow.queue_draw();
        self.settle();
    }

    fn settle(&self) {
        let (extent, room, arrow) = match self.horizontal() {
            true => (self.body.width(), self.window.width(), self.arrow.width()),
            false => (
                self.body.height(),
                self.window.height(),
                self.arrow.height(),
            ),
        };
        let (start, offset) = placement(self.center.get(), extent, room, arrow);
        match self.horizontal() {
            true => {
                self.slot.set_margin_start(start);
                self.arrow.set_margin_start(offset);
            }
            false => {
                self.slot.set_margin_top(start);
                self.arrow.set_margin_top(offset);
            }
        }
    }

    fn finish_close(&self) {
        self.release();
        self.window.set_visible(false);
    }

    fn release(&self) {
        if let Some(child) = self.body.first_child() {
            self.body.remove(&child);
        }
        self.slot.set_opacity(0.0);
        self.state.set(State::Closed);
        if let Some(dismissed) = self.dismissed.take() {
            dismissed();
        }
    }
}

fn placement(center: i32, extent: i32, room: i32, arrow: i32) -> (i32, i32) {
    let inset = arrow + arrow / 2;
    let near = arrow.min((center - inset).max(0));
    let far = (room - extent - arrow).max(center + inset - extent);
    let start = (center - extent / 2)
        .clamp(near, far.max(near))
        .clamp(0, (room - extent).max(0));
    let offset = (center - start - arrow / 2).clamp(arrow, (extent - 2 * arrow).max(arrow));
    (start, offset)
}

fn horizontal(side: Position) -> bool {
    matches!(side, Position::Top | Position::Bottom)
}

fn point(
    side: Position,
    area: &gtk4::DrawingArea,
    context: &gtk4::cairo::Context,
    width: i32,
    height: i32,
) {
    let (w, h) = (f64::from(width), f64::from(height));
    let tip = match side {
        Position::Top => [(w / 2.0, 0.0), (w, h), (0.0, h)],
        Position::Bottom => [(w / 2.0, h), (w, 0.0), (0.0, 0.0)],
        Position::Left => [(0.0, h / 2.0), (w, 0.0), (w, h)],
        Position::Right => [(w, h / 2.0), (0.0, 0.0), (0.0, h)],
    };
    let color = area.color();
    context.set_source_rgba(
        color.red().into(),
        color.green().into(),
        color.blue().into(),
        color.alpha().into(),
    );
    context.move_to(tip[0].0, tip[0].1);
    context.line_to(tip[1].0, tip[1].1);
    context.line_to(tip[2].0, tip[2].1);
    context.close_path();
    let _ = context.fill();
}

#[cfg(test)]
mod tests {
    use super::placement;

    const ARROW: i32 = 20;

    #[test]
    fn a_popover_centers_on_its_item() {
        let (start, offset) = placement(500, 400, 1920, ARROW);
        assert_eq!((start, offset), (300, 190));
        assert_eq!(start + offset + ARROW / 2, 500);
    }

    #[test]
    fn the_arrow_stays_on_the_item_when_the_body_cannot_center_on_it() {
        let (start, offset) = placement(28, 418, 1913, 18);
        assert_eq!(start + offset + 18 / 2, 28);
        assert!(
            offset >= 18,
            "the arrow rode the rounded corner at {offset}"
        );
    }

    #[test]
    fn a_popover_that_cannot_center_still_keeps_a_gutter_from_the_edge() {
        let (start, offset) = placement(100, 418, 1913, 18);
        assert_eq!(start, 18, "the body sat flush against the output edge");
        assert_eq!(start + offset + 18 / 2, 100);
    }

    #[test]
    fn a_popover_never_leaves_the_output() {
        for center in [0, 28, 500, 1900, 1920] {
            let (start, _) = placement(center, 400, 1920, ARROW);
            assert!(
                start >= 0 && start + 400 <= 1920,
                "center {center} put the body at {start}"
            );
        }
    }

    #[test]
    fn the_arrow_never_sits_on_the_bodys_rounded_corner() {
        for center in [0, 5, 28, 500, 1900, 1920] {
            let (_, offset) = placement(center, 400, 1920, ARROW);
            assert!(
                (ARROW..=400 - ARROW).contains(&offset),
                "center {center} put the arrow at {offset}"
            );
        }
    }

    #[test]
    fn an_unmeasured_popover_asks_for_nothing_out_of_range() {
        assert_eq!(placement(120, 0, 0, 0), (0, 0));
    }
}
