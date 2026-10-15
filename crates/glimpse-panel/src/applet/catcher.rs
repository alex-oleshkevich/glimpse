use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::AnimationExt;
use glimpse_config::Position;
use glimpse_widgets::Tip;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const SIDEWAYS: &str = "applet-popover__arrow--sideways";
const SHOWN: &str = "shown";
const SHOWN_OPACITY: f64 = 0.5;
const GUTTER: i32 = 40;

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
    popup: gtk4::Window,
    slot: gtk4::Grid,
    arrow: glimpse_widgets::PopoverArrow,
    body: gtk4::Box,
    fade: adw::TimedAnimation,
    side: Rc<Cell<Position>>,
    state: Cell<State>,
    center: Cell<i32>,
    inset: Cell<i32>,
    dismissed: RefCell<Option<Box<dyn Fn()>>>,
    shields: RefCell<Vec<gtk4::Window>>,
}

impl Catcher {
    pub fn new(monitor: Option<&gtk4::gdk::Monitor>, side: Position) -> Rc<Self> {
        let window = catching(monitor, 0);
        let popup = surface(monitor);

        let slot = gtk4::Grid::new();

        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        body.add_css_class("applet-popover__body");
        slot.attach(&body, 0, 1, 1, 1);

        let arrow = glimpse_widgets::PopoverArrow::new(tip(side));
        arrow.add_css_class("applet-popover__arrow");
        slot.attach(&arrow, 0, 0, 1, 1);

        popup.set_child(Some(&slot));
        shade(&popup, &body, &arrow, 0.0);

        let fade = adw::TimedAnimation::new(
            &slot,
            0.0,
            1.0,
            glimpse_widgets::duration_ms(),
            adw::CallbackAnimationTarget::new({
                let popup = popup.clone();
                let body = body.clone();
                let arrow = arrow.clone();
                move |value| shade(&popup, &body, &arrow, value)
            }),
        );
        fade.set_easing(adw::Easing::EaseOutCubic);

        let side = Rc::new(Cell::new(side));
        let catcher = Rc::new(Self {
            window: window.clone(),
            popup: popup.clone(),
            slot,
            arrow,
            body,
            fade,
            side: side.clone(),
            state: Cell::default(),
            center: Cell::default(),
            inset: Cell::default(),
            dismissed: RefCell::default(),
            shields: RefCell::default(),
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

        catcher.guard(&window);

        let press = gtk4::GestureClick::new();
        press.connect_pressed({
            let catcher = Rc::downgrade(&catcher);
            let popup = popup.clone();
            move |_, _, x, y| {
                let Some(catcher) = catcher.upgrade() else {
                    return;
                };
                let under = popup.pick(x, y, gtk4::PickFlags::DEFAULT);
                let inside = under.is_some_and(|widget| {
                    widget == catcher.slot || widget.is_ancestor(&catcher.slot)
                });
                if !inside {
                    catcher.close();
                }
            }
        });
        popup.add_controller(press);

        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let catcher = Rc::downgrade(&catcher);
            move |_, key, _, _| {
                if key != gtk4::gdk::Key::Escape {
                    return gtk4::glib::Propagation::Proceed;
                }
                if let Some(catcher) = catcher.upgrade() {
                    catcher.close();
                }
                gtk4::glib::Propagation::Stop
            }
        });
        popup.add_controller(keys);

        catcher.layout(side.get());

        catcher
    }

    pub fn open(
        self: &Rc<Self>,
        child: &impl IsA<gtk4::Widget>,
        center: i32,
        dismissed: impl Fn() + 'static,
    ) {
        self.release();
        self.set_typing(false);
        self.fade.reset();
        self.center.set(center);
        self.dismissed.replace(Some(Box::new(dismissed)));
        // A `GtkNative` owns its own surface and stays invisible until it is popped up, so
        // appending one parents it and shows nothing at all — no warning, no drawn pixel. Say so
        // rather than presenting an empty body: an applet wanting a popup of its own pops it.
        if child.as_ref().is::<gtk4::Native>() {
            tracing::error!("a popover child owning its own surface cannot be hosted; ignoring it");
            return;
        }
        self.body.append(child.as_ref());
        self.shield();
        self.state.set(State::Opening);
        shade(&self.popup, &self.body, &self.arrow, 0.0);
        self.window.present();
        self.popup.present();

        // A layer surface has no size and is not mapped until the compositor configures it, so
        // an idle right after present() has nothing to centre against and animates nothing. The
        // callback outlives the first placement because a drawer opening inside the body changes
        // what there is to place, and a body that grew against a fixed margin leaves the output.
        let catcher = Rc::downgrade(self);
        let placed = Cell::new((0, 0));
        self.slot.add_tick_callback(move |slot, _| {
            let Some(catcher) = catcher.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if matches!(catcher.state.get(), State::Closed | State::Closing) {
                return glib::ControlFlow::Break;
            }
            if catcher.room() == 0 || slot.width() == 0 {
                return glib::ControlFlow::Continue;
            }
            let measured = catcher.measured();
            if placed.replace(measured) != measured {
                catcher.settle();
            }
            if catcher.state.get() == State::Opening
                && catcher.fade.state() != adw::AnimationState::Playing
            {
                catcher.fade.set_duration(glimpse_widgets::duration_ms());
                catcher.fade.set_value_from(0.0);
                catcher.fade.set_value_to(1.0);
                catcher.fade.play();
            }
            glib::ControlFlow::Continue
        });
    }

    pub fn close(&self) {
        self.set_typing(false);
        if matches!(self.state.get(), State::Closed | State::Closing) {
            return;
        }
        let from = self.body.opacity();
        self.state.set(State::Closing);
        self.fade.reset();
        self.fade.set_duration(glimpse_widgets::duration_ms());
        self.fade.set_value_from(from);
        self.fade.set_value_to(0.0);
        self.fade.play();
    }

    pub fn set_typing(&self, typing: bool) {
        let wanted = match typing {
            true => KeyboardMode::Exclusive,
            false => KeyboardMode::None,
        };
        if self.popup.keyboard_mode() != wanted {
            self.popup.set_keyboard_mode(wanted);
        }
    }

    pub fn place(&self, center: i32) {
        self.center.set(center);
        self.settle();
    }

    pub fn holds(&self, widget: &impl IsA<gtk4::Widget>) -> bool {
        self.body.first_child().as_ref() == Some(widget.as_ref())
    }

    pub fn closing(&self) -> bool {
        matches!(self.state.get(), State::Closed | State::Closing)
    }

    pub fn horizontal(&self) -> bool {
        horizontal(self.side.get())
    }

    pub fn reconfigure(
        &self,
        monitor: &gtk4::gdk::Monitor,
        side: Position,
        inset: i32,
        blur: bool,
    ) {
        glimpse_widgets::blur::apply(&self.popup, blur);
        self.inset.set(inset);
        let moved = self.window.monitor().as_ref() != Some(monitor);
        if moved {
            self.window.set_monitor(Some(monitor));
            self.popup.set_monitor(Some(monitor));
        }
        if moved || side != self.side.get() {
            self.finish_close();
        }
        self.layout(side);
    }

    fn layout(&self, side: Position) {
        self.side.set(side);

        self.slot.set_halign(match side {
            Position::Right => gtk4::Align::End,
            _ => gtk4::Align::Start,
        });
        self.slot.set_valign(match side {
            Position::Bottom => gtk4::Align::End,
            _ => gtk4::Align::Start,
        });
        let (arrow, body) = match side {
            Position::Top => ((0, 0), (0, 1)),
            Position::Bottom => ((0, 1), (0, 0)),
            Position::Left => ((0, 0), (1, 0)),
            Position::Right => ((1, 0), (0, 0)),
        };
        self.cell(self.arrow.upcast_ref(), arrow);
        self.cell(self.body.upcast_ref(), body);

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
        let (bar, along) = match side {
            Position::Top => (Edge::Top, Edge::Left),
            Position::Bottom => (Edge::Bottom, Edge::Left),
            Position::Left => (Edge::Left, Edge::Top),
            Position::Right => (Edge::Right, Edge::Top),
        };
        for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            self.popup.set_anchor(edge, edge == bar || edge == along);
            self.popup.set_margin(edge, 0);
        }
        self.slot.set_margin_start(gutter(side == Position::Right));
        self.slot.set_margin_end(gutter(side == Position::Left));
        self.slot.set_margin_top(gutter(side == Position::Bottom));
        self.slot.set_margin_bottom(gutter(side == Position::Top));
        self.arrow.set_margin_start(0);
        self.arrow.set_margin_top(0);
        self.arrow.set_tip(tip(side));
        self.settle();
    }

    fn cell(&self, widget: &gtk4::Widget, (column, row): (i32, i32)) {
        let Some(child) = self
            .slot
            .layout_manager()
            .and_downcast::<gtk4::GridLayout>()
            .map(|manager| manager.layout_child(widget))
            .and_downcast::<gtk4::GridLayoutChild>()
        else {
            return;
        };
        child.set_column(column);
        child.set_row(row);
    }

    fn room(&self) -> i32 {
        self.measured().1
    }

    fn measured(&self) -> (i32, i32) {
        match self.horizontal() {
            true => (self.body.width(), self.window.width()),
            false => (self.body.height(), self.window.height()),
        }
    }

    fn settle(&self) {
        let (extent, room) = self.measured();
        if room == 0 {
            return;
        }

        let arrow = match self.horizontal() {
            true => self.arrow.width(),
            false => self.arrow.height(),
        };
        let center = self.center.get() + self.inset.get();
        let (start, offset) = placement(center, extent, room, arrow);
        let lead = start.min(GUTTER);
        let trail = (room - start - extent).clamp(0, GUTTER);
        match self.horizontal() {
            true => {
                self.popup.set_margin(Edge::Left, start - lead);
                self.slot.set_margin_start(lead);
                self.slot.set_margin_end(trail);
                self.arrow.set_margin_start(offset);
            }
            false => {
                self.popup.set_margin(Edge::Top, start - lead);
                self.slot.set_margin_top(lead);
                self.slot.set_margin_bottom(trail);
                self.arrow.set_margin_top(offset);
            }
        }
    }

    fn guard(self: &Rc<Self>, window: &gtk4::Window) {
        let canvas = gtk4::DrawingArea::new();
        canvas.set_draw_func(|_, _, _, _| {});
        window.set_child(Some(&canvas));
        let press = gtk4::GestureClick::new();
        press.connect_pressed({
            let catcher = Rc::downgrade(self);
            move |_, _, _, _| {
                if let Some(catcher) = catcher.upgrade() {
                    catcher.close();
                }
            }
        });
        window.add_controller(press);
    }

    fn shield(self: &Rc<Self>) {
        let Some(own) = self.window.monitor() else {
            return;
        };
        let shields = own
            .display()
            .monitors()
            .iter::<gtk4::gdk::Monitor>()
            .flatten()
            .filter(|monitor| *monitor != own)
            .map(|monitor| {
                let window = catching(Some(&monitor), -1);
                self.guard(&window);
                window.present();
                window
            })
            .collect();
        self.shields.replace(shields);
    }

    fn unshield(&self) {
        for shield in self.shields.take() {
            shield.destroy();
        }
    }

    fn finish_close(&self) {
        self.release();
        self.popup.set_visible(false);
        self.window.set_visible(false);
    }

    fn release(&self) {
        if let Some(child) = self.body.first_child() {
            self.body.remove(&child);
        }
        self.unshield();
        shade(&self.popup, &self.body, &self.arrow, 0.0);
        self.state.set(State::Closed);
        if let Some(dismissed) = self.dismissed.take() {
            dismissed();
        }
    }
}

impl Drop for Catcher {
    fn drop(&mut self) {
        self.unshield();
        self.popup.destroy();
        self.window.destroy();
    }
}

fn surface(monitor: Option<&gtk4::gdk::Monitor>) -> gtk4::Window {
    let window = gtk4::Window::new();
    window.init_layer_shell();
    window.set_namespace(Some("glimpse-popover"));
    window.set_layer(Layer::Top);
    window.set_keyboard_mode(KeyboardMode::None);
    window.add_css_class("popover-catcher");
    if let Some(monitor) = monitor {
        window.set_monitor(Some(monitor));
    }
    window
}

fn catching(monitor: Option<&gtk4::gdk::Monitor>, zone: i32) -> gtk4::Window {
    let window = surface(monitor);
    window.set_exclusive_zone(zone);
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        window.set_anchor(edge, true);
    }
    window
}

fn gutter(far: bool) -> i32 {
    match far {
        true => GUTTER,
        false => 0,
    }
}

fn shade(
    window: &gtk4::Window,
    body: &gtk4::Box,
    arrow: &glimpse_widgets::PopoverArrow,
    value: f64,
) {
    body.set_opacity(value);
    arrow.set_fill_opacity(value);
    let shown = value >= SHOWN_OPACITY;
    if window.has_css_class(SHOWN) != shown {
        match shown {
            true => window.add_css_class(SHOWN),
            false => window.remove_css_class(SHOWN),
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

fn tip(side: Position) -> Tip {
    match side {
        Position::Top => Tip::Up,
        Position::Bottom => Tip::Down,
        Position::Left => Tip::Left,
        Position::Right => Tip::Right,
    }
}

fn horizontal(side: Position) -> bool {
    matches!(side, Position::Top | Position::Bottom)
}

#[cfg(test)]
pub(crate) mod tests;
