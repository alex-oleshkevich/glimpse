use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gettextrs::{gettext, ngettext};
use glimpse_widgets::{Lens, zoomed};
use gtk4::{gdk, glib, prelude::*};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::capture::Frame;
use crate::marks::Marks;

const DEFAULT_ZOOM: u32 = 8;
const PILL_GAP: f64 = 10.0;
const RADIUS_STEP: f32 = 1.15;
const MIN_RADIUS: f32 = 40.0;
const MAX_RADIUS: f32 = 400.0;

fn resized(radius: f32, notches: f64) -> f32 {
    let next = if notches < 0.0 {
        (radius * RADIUS_STEP).max(radius + 4.0)
    } else if notches > 0.0 {
        (radius / RADIUS_STEP).min(radius - 4.0)
    } else {
        radius
    };
    next.clamp(MIN_RADIUS, MAX_RADIUS)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub radius: f32,
    pub max_zoom: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ended(Vec<Segment>),
    Invalidated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub from: (u32, u32),
    pub to: (u32, u32),
}

impl Segment {
    pub fn delta(self) -> (i64, i64) {
        (
            i64::from(self.to.0) - i64::from(self.from.0),
            i64::from(self.to.1) - i64::from(self.from.1),
        )
    }

    pub fn distance(self) -> f64 {
        let (dx, dy) = self.delta();
        (dx as f64).hypot(dy as f64)
    }

    pub fn angle(self) -> f64 {
        let (dx, dy) = self.delta();
        (dy.unsigned_abs() as f64)
            .atan2(dx.unsigned_abs() as f64)
            .to_degrees()
    }
}

#[derive(Debug, Default)]
struct Chain {
    segments: Vec<(usize, Segment)>,
    anchor: Option<(usize, (u32, u32))>,
}

impl Chain {
    fn place(&mut self, surface: usize, point: (u32, u32)) {
        if let Some((on, from)) = self.anchor
            && on == surface
        {
            self.segments.push((surface, Segment { from, to: point }));
        }
        self.anchor = Some((surface, point));
    }

    fn left(&mut self, surface: usize) {
        if self.anchor.is_some_and(|(on, _)| on == surface) {
            self.anchor = None;
        }
    }

    fn reset(&mut self) {
        self.segments.clear();
        self.anchor = None;
    }

    fn anchor_on(&self, surface: usize) -> Option<(u32, u32)> {
        self.anchor
            .filter(|(on, _)| *on == surface)
            .map(|(_, point)| point)
    }

    fn on(&self, surface: usize) -> Vec<Segment> {
        self.segments
            .iter()
            .filter(|(on, _)| *on == surface)
            .map(|(_, segment)| *segment)
            .collect()
    }

    fn confirmed(&self) -> Vec<Segment> {
        self.segments.iter().map(|(_, segment)| *segment).collect()
    }

    fn total(&self) -> Option<(f64, u32)> {
        (!self.segments.is_empty()).then(|| {
            (
                self.segments
                    .iter()
                    .map(|(_, segment)| segment.distance())
                    .sum(),
                self.segments.len() as u32,
            )
        })
    }
}

struct Surface {
    monitor: gdk::Monitor,
    window: gtk4::Window,
    lens: Lens,
    marks: Marks,
    layer: gtk4::Fixed,
    pill: gtk4::Box,
    lines: [gtk4::Label; 3],
    size: Cell<(f64, f64)>,
}

type Surfaces = Rc<Vec<Surface>>;
type Shared = Rc<RefCell<Chain>>;

pub struct Session {
    surfaces: Surfaces,
    invalidated: Vec<(gdk::Monitor, glib::SignalHandlerId)>,
}

impl Session {
    pub fn open(
        mut frames: Vec<Frame>,
        settings: Settings,
        done: impl Fn(Outcome) + 'static,
    ) -> Result<Self, String> {
        let display = gdk::Display::default().ok_or_else(|| "the display".to_owned())?;
        let zoom = Rc::new(Cell::new(DEFAULT_ZOOM.min(settings.max_zoom.max(1))));
        let radius = Rc::new(Cell::new(settings.radius.clamp(MIN_RADIUS, MAX_RADIUS)));
        let mut surfaces: Vec<Surface> = Vec::new();
        for monitor in display.monitors().iter::<gdk::Monitor>().flatten() {
            let connector = monitor
                .connector()
                .map(|name| name.to_string())
                .unwrap_or_default();
            let Some(index) = frames.iter().position(|frame| frame.connector == connector) else {
                for surface in &surfaces {
                    surface.window.destroy();
                }
                return Err(format!("output {connector:?}"));
            };
            surfaces.push(surface(
                &monitor,
                frames.swap_remove(index),
                zoom.get(),
                settings.radius,
            ));
        }
        if surfaces.is_empty() {
            return Err("any output".to_owned());
        }

        let surfaces = Rc::new(surfaces);
        let chain: Shared = Rc::default();
        let finished = Cell::new(false);
        let finish: Rc<dyn Fn(Outcome)> = Rc::new(move |outcome| {
            if !finished.replace(true) {
                done(outcome);
            }
        });

        for index in 0..surfaces.len() {
            connect(
                &surfaces,
                index,
                chain.clone(),
                zoom.clone(),
                radius.clone(),
                settings,
                finish.clone(),
            );
        }
        let mut invalidated = Vec::new();
        for surface in surfaces.iter() {
            let handler = surface.monitor.connect_invalidate({
                let finish = finish.clone();
                move |_| finish(Outcome::Invalidated)
            });
            invalidated.push((surface.monitor.clone(), handler));
            surface.window.present();
        }
        Ok(Self {
            surfaces,
            invalidated,
        })
    }

    pub fn close(self) {
        for (monitor, handler) in self.invalidated {
            monitor.disconnect(handler);
        }
        for surface in self.surfaces.iter() {
            surface.window.destroy();
        }
    }
}

fn surface(monitor: &gdk::Monitor, frame: Frame, zoom: u32, radius: f32) -> Surface {
    let window = gtk4::Window::new();
    window.init_layer_shell();
    window.set_namespace(Some("glimpse-ruler"));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_exclusive_zone(-1);
    window.set_monitor(Some(monitor));
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.add_css_class("ruler");
    window.set_cursor(gdk::Cursor::from_name("none", None).as_ref());

    let texture: gdk::Texture = gdk::MemoryTexture::new(
        frame.width as i32,
        frame.height as i32,
        gdk::MemoryFormat::R8g8b8x8,
        &frame.pixels,
        frame.width as usize * 4,
    )
    .upcast();
    let buffer = (frame.width, frame.height);
    let lens = Lens::new(&texture, buffer, zoom, radius);
    let marks = Marks::new(buffer, radius);
    let lines = [(); 3].map(|_| {
        let label = gtk4::Label::new(None);
        label.add_css_class("ruler__value");
        label.set_xalign(0.0);
        label
    });
    lines[1].add_css_class("ruler__value--muted");
    let pill = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    pill.add_css_class("ruler__pill");
    pill.set_visible(false);
    for line in &lines {
        pill.append(line);
    }
    let layer = gtk4::Fixed::new();
    layer.set_can_target(false);
    layer.put(&pill, 0.0, 0.0);

    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&lens));
    overlay.add_overlay(&marks);
    overlay.add_overlay(&layer);
    window.set_child(Some(&overlay));

    Surface {
        monitor: monitor.clone(),
        window,
        lens,
        marks,
        layer,
        pill,
        lines,
        size: Cell::new((0.0, 0.0)),
    }
}

fn readout(
    anchor: Option<(u32, u32)>,
    live: (u32, u32),
    total: Option<(f64, u32)>,
) -> [Option<String>; 3] {
    let chained = total.is_some();
    let total = total.map(|(length, count)| {
        ngettext(
            "total {length}px, {count} segment",
            "total {length}px, {count} segments",
            count,
        )
        .replace("{length}", &format!("{length:.1}"))
        .replace("{count}", &count.to_string())
    });
    let Some(from) = anchor else {
        return [Some(format!("{}, {}", live.0, live.1)), None, total];
    };
    let segment = Segment { from, to: live };
    let (dx, dy) = segment.delta();
    let length = match chained {
        true => gettext("segment {length}px · {angle}°"),
        false => "{length}px · {angle}°".to_owned(),
    }
    .replace("{length}", &format!("{:.1}", segment.distance()))
    .replace("{angle}", &format!("{:.1}", segment.angle()));
    [Some(format!("Δx {dx}  Δy {dy}")), Some(length), total]
}

fn constrain(anchor: Option<(u32, u32)>, point: (u32, u32), locked: bool) -> (u32, u32) {
    let Some(anchor) = anchor.filter(|_| locked) else {
        return point;
    };
    if point.0.abs_diff(anchor.0) >= point.1.abs_diff(anchor.1) {
        (point.0, anchor.1)
    } else {
        (anchor.0, point.1)
    }
}

fn dress(surface: &Surface, index: usize, chain: &Chain, radius: f32, shift: bool) {
    let pointer = surface.lens.pointer();
    let anchor = chain.anchor_on(index);
    let live = pointer.map(|_| constrain(anchor, surface.lens.pixel(), shift));
    surface.marks.set(chain.on(index), anchor, live, pointer);
    let (Some((x, y)), Some(live)) = (pointer, live) else {
        surface.pill.set_visible(false);
        return;
    };
    surface.pill.set_visible(true);
    let mut changed = false;
    for (label, text) in surface
        .lines
        .iter()
        .zip(readout(anchor, live, chain.total()))
    {
        label.set_visible(text.is_some());
        let text = text.unwrap_or_default();
        if label.text() != text {
            label.set_text(&text);
            changed = true;
        }
    }
    if changed {
        let (_, width, _, _) = surface.pill.measure(gtk4::Orientation::Horizontal, -1);
        let (_, height, _, _) = surface.pill.measure(gtk4::Orientation::Vertical, width);
        surface.size.set((f64::from(width), f64::from(height)));
    }
    let (left, top) = pill_origin(
        (x, y),
        f64::from(radius),
        surface.size.get(),
        (
            f64::from(surface.lens.width()),
            f64::from(surface.lens.height()),
        ),
    );
    surface.layer.move_(&surface.pill, left, top);
}

fn pill_origin(pointer: (f64, f64), radius: f64, pill: (f64, f64), area: (f64, f64)) -> (f64, f64) {
    let below = pointer.1 + radius + PILL_GAP;
    let top = match below + pill.1 > area.1 {
        true => pointer.1 - radius - PILL_GAP - pill.1,
        false => below,
    };
    let left = (pointer.0 - pill.0 / 2.0).clamp(0.0, (area.0 - pill.0).max(0.0));
    (left.round(), top.max(0.0).round())
}

fn place(surfaces: &Surfaces, index: usize, chain: &Shared, radius: f32, shift: bool) {
    let surface = &surfaces[index];
    let anchor = chain.borrow().anchor_on(index);
    let point = constrain(anchor, surface.lens.pixel(), shift);
    chain.borrow_mut().place(index, point);
    dress(surface, index, &chain.borrow(), radius, shift);
}

fn connect(
    surfaces: &Surfaces,
    index: usize,
    chain: Shared,
    zoom: Rc<Cell<u32>>,
    radius: Rc<Cell<f32>>,
    settings: Settings,
    finish: Rc<dyn Fn(Outcome)>,
) {
    let surface = &surfaces[index];
    let weak = Rc::downgrade(surfaces);
    let end = {
        let chain = chain.clone();
        move || finish(Outcome::Ended(chain.borrow().confirmed()))
    };

    let motion = gtk4::EventControllerMotion::new();
    motion.connect_enter({
        let weak = weak.clone();
        let chain = chain.clone();
        let radius = radius.clone();
        move |controller, x, y| {
            let shift = controller
                .current_event_state()
                .contains(gdk::ModifierType::SHIFT_MASK);
            moved(&weak, &chain, index, x, y, radius.get(), shift)
        }
    });
    motion.connect_motion({
        let weak = weak.clone();
        let chain = chain.clone();
        let radius = radius.clone();
        move |controller, x, y| {
            let shift = controller
                .current_event_state()
                .contains(gdk::ModifierType::SHIFT_MASK);
            moved(&weak, &chain, index, x, y, radius.get(), shift)
        }
    });
    motion.connect_leave({
        let weak = weak.clone();
        let chain = chain.clone();
        let radius = radius.clone();
        move |_| {
            if let Some(surfaces) = weak.upgrade() {
                left(&surfaces, &chain, index, radius.get());
            }
        }
    });
    surface.lens.add_controller(motion);

    let click = gtk4::GestureClick::new();
    click.set_button(0);
    click.connect_pressed({
        let weak = weak.clone();
        let chain = chain.clone();
        let radius = radius.clone();
        move |gesture, _, x, y| {
            let Some(surfaces) = weak.upgrade() else {
                return;
            };
            let shift = gesture
                .current_event_state()
                .contains(gdk::ModifierType::SHIFT_MASK);
            match gesture.current_button() {
                gdk::BUTTON_PRIMARY => {
                    if !surfaces[index].lens.hovered() {
                        surfaces[index].lens.point(x, y);
                    }
                    place(&surfaces, index, &chain, radius.get(), shift);
                }
                gdk::BUTTON_SECONDARY => {
                    chain.borrow_mut().reset();
                    for (other, surface) in surfaces.iter().enumerate() {
                        dress(surface, other, &chain.borrow(), radius.get(), shift);
                    }
                }
                _ => {}
            }
        }
    });
    surface.lens.add_controller(click);

    let scroll = gtk4::EventControllerScroll::new(
        gtk4::EventControllerScrollFlags::VERTICAL | gtk4::EventControllerScrollFlags::DISCRETE,
    );
    scroll.connect_scroll({
        let weak = weak.clone();
        let chain = chain.clone();
        let radius = radius.clone();
        move |controller, _, dy| {
            let Some(surfaces) = weak.upgrade() else {
                return glib::Propagation::Stop;
            };
            let shift = controller
                .current_event_state()
                .contains(gdk::ModifierType::SHIFT_MASK);
            if shift {
                let next = resized(radius.get(), dy);
                radius.set(next);
                for surface in surfaces.iter() {
                    surface.lens.set_radius(next);
                    surface.marks.set_radius(next);
                }
                dress(&surfaces[index], index, &chain.borrow(), next, false);
            } else {
                let next = zoomed(zoom.get(), dy, settings.max_zoom);
                zoom.set(next);
                for surface in surfaces.iter() {
                    surface.lens.set_zoom(next);
                }
            }
            glib::Propagation::Stop
        }
    });
    surface.lens.add_controller(scroll);

    let keys = gtk4::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, state| {
        let Some(surfaces) = weak.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let Some(hovered) = surfaces.iter().position(|surface| surface.lens.hovered()) else {
            if key == gdk::Key::Escape {
                end();
            }
            return glib::Propagation::Stop;
        };
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        let step = |dx, dy| {
            surfaces[hovered].lens.nudge(dx, dy);
            dress(
                &surfaces[hovered],
                hovered,
                &chain.borrow(),
                radius.get(),
                shift,
            );
        };
        match key {
            gdk::Key::Escape => end(),
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space => {
                place(&surfaces, hovered, &chain, radius.get(), shift)
            }
            gdk::Key::Left => step(-1, 0),
            gdk::Key::Right => step(1, 0),
            gdk::Key::Up => step(0, -1),
            gdk::Key::Down => step(0, 1),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    });
    surface.window.add_controller(keys);
}

fn left(surfaces: &[Surface], chain: &Shared, index: usize, radius: f32) {
    surfaces[index].lens.leave();
    chain.borrow_mut().left(index);
    dress(&surfaces[index], index, &chain.borrow(), radius, false);
}

fn moved(
    weak: &Weak<Vec<Surface>>,
    chain: &Shared,
    index: usize,
    x: f64,
    y: f64,
    radius: f32,
    shift: bool,
) {
    let Some(surfaces) = weak.upgrade() else {
        return;
    };
    for (other, surface) in surfaces.iter().enumerate() {
        if other != index && surface.lens.hovered() {
            left(&surfaces, chain, other, radius);
        }
    }
    surfaces[index].lens.point(x, y);
    dress(&surfaces[index], index, &chain.borrow(), radius, shift);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconstrained_or_anchorless_points_pass_through_unchanged() {
        assert_eq!(constrain(None, (10, 20), true), (10, 20));
        assert_eq!(constrain(Some((5, 5)), (10, 20), false), (10, 20));
    }

    #[test]
    fn constrained_snaps_to_whichever_axis_moved_further() {
        assert_eq!(constrain(Some((100, 100)), (140, 110), true), (140, 100));
        assert_eq!(constrain(Some((100, 100)), (110, 140), true), (100, 140));
    }

    #[test]
    fn a_tie_locks_horizontal() {
        assert_eq!(constrain(Some((100, 100)), (140, 140), true), (140, 100));
    }

    #[test]
    fn resizing_steps_in_both_directions_and_clamps_to_the_documented_range() {
        let grown = resized(100.0, -1.0);
        assert!(grown > 100.0);
        let shrunk = resized(100.0, 1.0);
        assert!(shrunk < 100.0);
        assert_eq!(resized(MIN_RADIUS, 1.0), MIN_RADIUS);
        assert_eq!(resized(MAX_RADIUS, -1.0), MAX_RADIUS);
        assert_eq!(resized(100.0, 0.0), 100.0);
    }

    #[test]
    fn distance_is_straight_and_the_angle_is_measured_from_horizontal_without_a_sign() {
        let segment = Segment {
            from: (412, 268),
            to: (626, 364),
        };
        assert_eq!(segment.delta(), (214, 96));
        assert!((segment.distance() - 234.546).abs() < 0.001);
        assert!((segment.angle() - 24.1609).abs() < 0.001);

        let back = Segment {
            from: segment.to,
            to: segment.from,
        };
        assert_eq!(back.delta(), (-214, -96));
        assert_eq!(back.distance(), segment.distance());
        assert_eq!(back.angle(), segment.angle());

        let flat = Segment {
            from: (10, 5),
            to: (0, 5),
        };
        assert_eq!(flat.angle(), 0.0);
        let upright = Segment {
            from: (5, 10),
            to: (5, 0),
        };
        assert_eq!(upright.angle(), 90.0);
        let still = Segment {
            from: (5, 5),
            to: (5, 5),
        };
        assert_eq!((still.distance(), still.angle()), (0.0, 0.0));
    }

    #[test]
    fn the_first_point_anchors_and_each_later_one_confirms_a_segment_and_chains_on() {
        let mut chain = Chain::default();
        chain.place(0, (1, 1));
        assert!(chain.confirmed().is_empty());
        assert_eq!(chain.total(), None);

        chain.place(0, (4, 5));
        chain.place(0, (4, 15));

        assert_eq!(
            chain.confirmed(),
            vec![
                Segment {
                    from: (1, 1),
                    to: (4, 5)
                },
                Segment {
                    from: (4, 5),
                    to: (4, 15)
                },
            ]
        );
        assert_eq!(chain.anchor_on(0), Some((4, 15)));
        assert_eq!(chain.total(), Some((15.0, 2)));
    }

    #[test]
    fn leaving_an_output_drops_its_anchor_but_keeps_what_was_confirmed() {
        let mut chain = Chain::default();
        chain.place(0, (0, 0));
        chain.place(0, (3, 4));
        chain.left(1);
        assert_eq!(chain.anchor_on(0), Some((3, 4)));

        chain.left(0);
        assert_eq!(chain.anchor_on(0), None);
        chain.place(1, (9, 9));
        chain.place(1, (9, 19));

        assert_eq!(chain.on(0).len(), 1);
        assert_eq!(chain.on(1).len(), 1);
        assert_eq!(chain.confirmed().len(), 2);
    }

    #[test]
    fn a_point_on_another_output_never_joins_the_anchor_across_outputs() {
        let mut chain = Chain::default();
        chain.place(0, (5, 5));
        chain.place(1, (6, 6));

        assert!(chain.confirmed().is_empty());
        assert_eq!(chain.anchor_on(1), Some((6, 6)));
    }

    #[test]
    fn reset_discards_every_confirmed_segment_and_the_pending_anchor() {
        let mut chain = Chain::default();
        chain.place(0, (0, 0));
        chain.place(0, (3, 4));
        chain.place(0, (10, 10));

        chain.reset();

        assert!(chain.confirmed().is_empty());
        assert_eq!(chain.anchor_on(0), None);
        assert_eq!(chain.total(), None);
    }

    #[test]
    fn the_readout_shows_the_position_idle_and_the_segment_once_anchored() {
        assert_eq!(
            readout(None, (412, 268), None),
            [Some("412, 268".to_owned()), None, None]
        );
        assert_eq!(
            readout(Some((412, 268)), (626, 364), None),
            [
                Some("Δx 214  Δy 96".to_owned()),
                Some("234.5px · 24.2°".to_owned()),
                None,
            ]
        );
        assert_eq!(
            readout(Some((412, 268)), (626, 364), Some((12.34, 1))),
            [
                Some("Δx 214  Δy 96".to_owned()),
                Some("segment 234.5px · 24.2°".to_owned()),
                Some("total 12.3px, 1 segment".to_owned()),
            ]
        );
        assert_eq!(
            readout(None, (1, 2), Some((195.3, 2)))[2],
            Some("total 195.3px, 2 segments".to_owned())
        );
    }

    #[test]
    fn the_readout_sits_under_the_lens_and_flips_above_it_near_the_bottom_edge() {
        let area = (1000.0, 800.0);
        let pill = (120.0, 34.0);

        assert_eq!(
            pill_origin((500.0, 300.0), 88.0, pill, area),
            (440.0, 398.0)
        );
        assert_eq!(
            pill_origin((500.0, 780.0), 88.0, pill, area),
            (440.0, 648.0)
        );
        assert_eq!(pill_origin((10.0, 300.0), 88.0, pill, area), (0.0, 398.0));
        assert_eq!(
            pill_origin((995.0, 300.0), 88.0, pill, area),
            (880.0, 398.0)
        );
    }
}
