use std::cell::{Cell, OnceCell, RefCell};
use std::ffi::c_void;
use std::rc::{Rc, Weak};

use gtk4::glib::translate::ToGlibPtr;
use gtk4::prelude::*;
use gtk4::{gdk, glib, graphene, gsk};
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::{
    self, Capability, ExtBackgroundEffectManagerV1,
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

const BLURRED: &str = "blurred";
const SHOWN_OPACITY: f64 = 0.5;

unsafe extern "C" {
    fn gdk_wayland_display_get_wl_display(display: *mut gdk::ffi::GdkDisplay) -> *mut c_void;
    fn gdk_wayland_surface_get_wl_surface(surface: *mut gdk::ffi::GdkSurface) -> *mut c_void;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tip {
    Up,
    Down,
    Left,
    Right,
}

pub enum Shape {
    Surface(gtk4::Widget),
    Arrow(gtk4::Widget, Tip),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

type Corners = [(f32, f32); 4];

const SQUARE: Corners = [(0.0, 0.0); 4];

#[derive(Default)]
struct Events {
    blur: bool,
}

impl Dispatch<WlRegistry, GlobalListContents> for Events {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtBackgroundEffectManagerV1, ()> for Events {
    fn event(
        events: &mut Self,
        _: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            events.blur = matches!(flags, WEnum::Value(flags) if flags.contains(Capability::Blur));
        }
    }
}

delegate_noop!(Events: ignore WlCompositor);
delegate_noop!(Events: ignore WlRegion);
delegate_noop!(Events: ignore ExtBackgroundEffectSurfaceV1);

struct Protocol {
    connection: Connection,
    queue: RefCell<EventQueue<Events>>,
    events: RefCell<Events>,
    handle: QueueHandle<Events>,
    compositor: WlCompositor,
    manager: ExtBackgroundEffectManagerV1,
}

impl Protocol {
    fn bind() -> Option<&'static Self> {
        let display = gdk::Display::default()?;
        if display.type_().name() != "GdkWaylandDisplay" {
            tracing::debug!("not a Wayland display; blur is unavailable");
            return None;
        }
        let pointer = unsafe { gdk_wayland_display_get_wl_display(display.to_glib_none().0) };
        if pointer.is_null() {
            return None;
        }
        let backend = unsafe { Backend::from_foreign_display(pointer.cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, mut queue) = registry_queue_init::<Events>(&connection)
            .inspect_err(
                |error| tracing::debug!(%error, "cannot list globals; blur is unavailable"),
            )
            .ok()?;
        let handle = queue.handle();
        let compositor = globals
            .bind::<WlCompositor, _, _>(&handle, 1..=6, ())
            .ok()?;
        let manager = globals
            .bind::<ExtBackgroundEffectManagerV1, _, _>(&handle, 1..=1, ())
            .inspect_err(
                |error| tracing::debug!(%error, "no background effect; blur is unavailable"),
            )
            .ok()?;
        let mut events = Events::default();
        queue.roundtrip(&mut events).ok()?;
        tracing::debug!(blur = events.blur, "background effect bound");
        Some(Box::leak(Box::new(Self {
            connection,
            queue: RefCell::new(queue),
            events: RefCell::new(events),
            handle,
            compositor,
            manager,
        })))
    }

    fn get() -> Option<&'static Self> {
        thread_local! {
            static PROTOCOL: OnceCell<Option<&'static Protocol>> = const { OnceCell::new() };
        }
        PROTOCOL
            .try_with(|cell| *cell.get_or_init(Self::bind))
            .ok()
            .flatten()
    }

    fn blurs(&self) -> bool {
        let mut events = self.events.borrow_mut();
        if let Err(error) = self.queue.borrow_mut().dispatch_pending(&mut events) {
            tracing::debug!(%error, "background effect events");
        }
        events.blur
    }

    fn effect(&self, surface: &gdk::Surface) -> Option<(ExtBackgroundEffectSurfaceV1, WlSurface)> {
        let pointer = unsafe { gdk_wayland_surface_get_wl_surface(surface.to_glib_none().0) };
        if pointer.is_null() {
            return None;
        }
        let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), pointer.cast()) }.ok()?;
        let surface = WlSurface::from_id(&self.connection, id).ok()?;
        let effect = self
            .manager
            .get_background_effect(&surface, &self.handle, ());
        Some((effect, surface))
    }

    fn send(&self, effect: &ExtBackgroundEffectSurfaceV1, rects: Option<&[Rect]>) {
        match rects {
            Some(rects) => {
                let region = self.compositor.create_region(&self.handle, ());
                for rect in rects {
                    region.add(rect.x, rect.y, rect.width, rect.height);
                }
                effect.set_blur_region(Some(&region));
                region.destroy();
            }
            None => effect.set_blur_region(None),
        }
    }

    fn flush(&self) {
        if let Err(error) = self.connection.flush() {
            tracing::debug!(%error, "flushing the background effect");
        }
    }
}

struct Attached {
    effect: ExtBackgroundEffectSurfaceV1,
    surface: WlSurface,
    clock: gdk::FrameClock,
    handlers: [glib::SignalHandlerId; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Layout,
    Painted,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Piece {
    Rounded(Rect, Corners),
    Arrow(Rect, Tip),
}

impl Piece {
    fn strips(self) -> Vec<Rect> {
        match self {
            Self::Rounded(rect, corners) => rounded(rect, corners),
            Self::Arrow(rect, tip) => arrow(rect, tip),
        }
    }
}

struct Inner {
    window: gtk4::Window,
    shapes: Box<dyn Fn() -> Vec<Shape>>,
    enabled: Cell<bool>,
    attached: RefCell<Option<Attached>>,
    signals: RefCell<Vec<glib::SignalHandlerId>>,
    sent: RefCell<Option<Option<Vec<Piece>>>>,
    uncommitted: Cell<bool>,
    corners: RefCell<Vec<(gtk4::Widget, Corners)>>,
    stale: Cell<bool>,
    syncing: Cell<bool>,
}

thread_local! {
    static LIVE: RefCell<Vec<Weak<Inner>>> = const { RefCell::new(Vec::new()) };
}

pub struct Blur(Rc<Inner>);

impl Blur {
    pub fn attach(
        window: &impl IsA<gtk4::Window>,
        shapes: impl Fn() -> Vec<Shape> + 'static,
    ) -> Self {
        let window = window.as_ref().clone();
        let inner = Rc::new(Inner {
            window: window.clone(),
            shapes: Box::new(shapes),
            enabled: Cell::new(false),
            attached: RefCell::default(),
            signals: RefCell::default(),
            sent: RefCell::default(),
            uncommitted: Cell::new(false),
            corners: RefCell::default(),
            stale: Cell::new(false),
            syncing: Cell::new(false),
        });
        let mapped = window.connect_map(glib::clone!(
            #[weak]
            inner,
            move |_| inner.sync()
        ));
        let unmapped = window.connect_unmap(glib::clone!(
            #[weak]
            inner,
            move |_| inner.sync()
        ));
        inner.signals.replace(vec![mapped, unmapped]);
        LIVE.with(|live| {
            let mut live = live.borrow_mut();
            live.retain(|entry| entry.strong_count() > 0);
            live.push(Rc::downgrade(&inner));
        });
        Self(inner)
    }

    pub fn set_enabled(&self, enabled: bool) {
        if self.0.enabled.replace(enabled) == enabled {
            return;
        }
        self.0.sync();
    }
}

impl Drop for Blur {
    fn drop(&mut self) {
        self.0.detach();
        for signal in self.0.signals.take() {
            self.0.window.disconnect(signal);
        }
        self.0.window.remove_css_class(BLURRED);
    }
}

pub(crate) fn restyle() {
    LIVE.with(|live| {
        live.borrow_mut().retain(|entry| entry.strong_count() > 0);
        for inner in live.borrow().iter().filter_map(Weak::upgrade) {
            inner.stale.set(true);
            inner.window.queue_draw();
        }
    });
}

/// `map`/`unmap` and a config reload can each ask `Inner::sync` to run, and GTK is free to nest
/// one inside the other — a monitor change applied while a popover is presenting, say. `attach`
/// and `detach` are reachable only through `sync`, so this guard is what stops a nested call from
/// taking a second `RefCell` borrow the outer call already holds and aborting the process; GTK
/// invokes this from a `g_signal_emit` trampoline that cannot unwind a Rust panic.
struct SyncGuard<'a>(&'a Cell<bool>);

impl Drop for SyncGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl Inner {
    fn sync(self: &Rc<Self>) {
        if self.syncing.replace(true) {
            return;
        }
        let _guard = SyncGuard(&self.syncing);

        let protocol = Protocol::get();
        let blurs = protocol.as_ref().is_some_and(|protocol| protocol.blurs());
        let active = self.enabled.get() && blurs;
        if self.window.has_css_class(BLURRED) != active {
            match active {
                true => self.window.add_css_class(BLURRED),
                false => self.window.remove_css_class(BLURRED),
            }
        }
        let wanted = self.enabled.get() && self.window.is_mapped();
        let attached = self.attached.borrow().is_some();
        match (wanted, attached) {
            (true, false) => self.attach(),
            (false, true) => self.detach(),
            _ => {}
        }
        self.window.queue_draw();
    }

    fn attach(self: &Rc<Self>) {
        let Some(protocol) = Protocol::get() else {
            return;
        };
        let Some(surface) = self.window.surface() else {
            return;
        };
        let Some((effect, wl_surface)) = protocol.effect(&surface) else {
            return;
        };
        let clock = surface.frame_clock();
        let layout = clock.connect_layout(glib::clone!(
            #[weak(rename_to = inner)]
            self,
            move |_| inner.update(Phase::Layout)
        ));
        let painted = clock.connect_after_paint(glib::clone!(
            #[weak(rename_to = inner)]
            self,
            move |_| inner.update(Phase::Painted)
        ));
        self.sent.take();
        self.uncommitted.set(false);
        self.corners.borrow_mut().clear();
        self.attached.replace(Some(Attached {
            effect,
            surface: wl_surface,
            clock,
            handlers: [layout, painted],
        }));
    }

    fn detach(&self) {
        let Some(attached) = self.attached.take() else {
            return;
        };
        for handler in attached.handlers {
            attached.clock.disconnect(handler);
        }
        attached.effect.destroy();
        if let Some(protocol) = Protocol::get() {
            protocol.flush();
        }
        self.sent.take();
        self.uncommitted.set(false);
        self.corners.borrow_mut().clear();
    }

    fn update(&self, phase: Phase) {
        let Some(protocol) = Protocol::get() else {
            return;
        };
        let attached = self.attached.borrow();
        let Some(attached) = attached.as_ref() else {
            return;
        };
        let painted = phase == Phase::Painted;
        if painted && self.stale.replace(false) {
            self.corners.borrow_mut().clear();
        }
        let wanted = (self.enabled.get() && protocol.blurs()).then(|| self.pieces(painted));
        if self.sent.borrow().as_ref() != Some(&wanted) {
            let strips = wanted.as_ref().map(|pieces| {
                pieces
                    .iter()
                    .flat_map(|piece| piece.strips())
                    .collect::<Vec<_>>()
            });
            tracing::debug!(rects = strips.as_ref().map(Vec::len), ?phase, "blur region");
            protocol.send(&attached.effect, strips.as_deref());
            self.sent.replace(Some(wanted));
            self.uncommitted.set(true);
        }
        if painted && self.uncommitted.replace(false) {
            attached.surface.commit();
        }
        protocol.flush();
    }

    fn pieces(&self, painted: bool) -> Vec<Piece> {
        let (dx, dy) = self.window.surface_transform();
        let shapes = (self.shapes)();
        self.corners.borrow_mut().retain(|(known, _)| {
            shapes.iter().any(|shape| match shape {
                Shape::Surface(widget) | Shape::Arrow(widget, _) => widget == known,
            })
        });
        shapes
            .into_iter()
            .filter_map(|shape| {
                let (widget, tip) = match shape {
                    Shape::Surface(widget) => (widget, None),
                    Shape::Arrow(widget, tip) => (widget, Some(tip)),
                };
                if !shown(&widget, &self.window) {
                    return None;
                }
                let rect = enclosing(&widget.compute_bounds(&self.window)?, dx, dy);
                match tip {
                    Some(tip) => Some(Piece::Arrow(rect, tip)),
                    None => Some(Piece::Rounded(rect, self.corners_of(&widget, painted)?)),
                }
            })
            .collect()
    }

    fn corners_of(&self, widget: &gtk4::Widget, painted: bool) -> Option<Corners> {
        if let Some((_, corners)) = self
            .corners
            .borrow()
            .iter()
            .find(|(known, _)| known == widget)
        {
            return Some(*corners);
        }
        if !painted {
            return None;
        }
        let corners = corners(widget)?;
        self.corners.borrow_mut().push((widget.clone(), corners));
        Some(corners)
    }
}

fn shown(widget: &gtk4::Widget, window: &gtk4::Window) -> bool {
    if !widget.is_mapped() {
        return false;
    }
    let mut opacity = 1.0;
    let mut current = Some(widget.clone());
    while let Some(widget) = current {
        opacity *= widget.opacity();
        if opacity < SHOWN_OPACITY {
            return false;
        }
        if widget == *window.upcast_ref::<gtk4::Widget>() {
            return true;
        }
        current = widget.parent();
    }
    false
}

fn enclosing(bounds: &graphene::Rect, dx: f64, dy: f64) -> Rect {
    let left = (f64::from(bounds.x()) + dx).floor();
    let top = (f64::from(bounds.y()) + dy).floor();
    let right = (f64::from(bounds.x() + bounds.width()) + dx).ceil();
    let bottom = (f64::from(bounds.y() + bounds.height()) + dy).ceil();
    Rect {
        x: left as i32,
        y: top as i32,
        width: (right - left) as i32,
        height: (bottom - top) as i32,
    }
}

fn corners(widget: &gtk4::Widget) -> Option<Corners> {
    let bounds = widget.compute_bounds(widget)?;
    let (width, height) = (bounds.width(), bounds.height());
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let paintable = gtk4::WidgetPaintable::new(Some(widget));
    let (intrinsic_width, intrinsic_height) =
        (paintable.intrinsic_width(), paintable.intrinsic_height());
    if intrinsic_width != width.ceil() as i32 || intrinsic_height != height.ceil() as i32 {
        return None;
    }
    let snapshot = gtk4::Snapshot::new();
    paintable.snapshot(
        &snapshot,
        f64::from(intrinsic_width),
        f64::from(intrinsic_height),
    );
    let node = snapshot.to_node()?;
    Some(outline(&node, width, height).unwrap_or(SQUARE))
}

fn outline(node: &gsk::RenderNode, width: f32, height: f32) -> Option<Corners> {
    let own = |rect: &gsk::RoundedRect| {
        let bounds = rect.bounds();
        ((bounds.width() - width).abs() <= 0.5 && (bounds.height() - height).abs() <= 0.5)
            .then(|| rect.corner().map(|size| (size.width(), size.height())))
    };
    if let Some(clip) = node.downcast_ref::<gsk::RoundedClipNode>()
        && let Some(corners) = own(&clip.clip())
    {
        return Some(corners);
    }
    if let Some(shadow) = node.downcast_ref::<gsk::OutsetShadowNode>()
        && let Some(corners) = own(&shadow.outline())
    {
        return Some(corners);
    }
    children(node)
        .iter()
        .find_map(|child| outline(child, width, height))
}

fn children(node: &gsk::RenderNode) -> Vec<gsk::RenderNode> {
    if let Some(container) = node.downcast_ref::<gsk::ContainerNode>() {
        return (0..container.n_children())
            .map(|index| container.child(index))
            .collect();
    }
    let child = if let Some(node) = node.downcast_ref::<gsk::TransformNode>() {
        node.child()
    } else if let Some(node) = node.downcast_ref::<gsk::OpacityNode>() {
        node.child()
    } else if let Some(node) = node.downcast_ref::<gsk::ClipNode>() {
        node.child()
    } else if let Some(node) = node.downcast_ref::<gsk::RoundedClipNode>() {
        node.child()
    } else if let Some(node) = node.downcast_ref::<gsk::DebugNode>() {
        node.child()
    } else if let Some(node) = node.downcast_ref::<gsk::ShadowNode>() {
        node.child()
    } else if let Some(node) = node.downcast_ref::<gsk::ColorMatrixNode>() {
        node.child()
    } else {
        return Vec::new();
    };
    vec![child]
}

fn rounded(rect: Rect, corners: Corners) -> Vec<Rect> {
    let clamp = |(rx, ry): (f32, f32)| {
        (
            rx.clamp(0.0, rect.width as f32 / 2.0),
            ry.clamp(0.0, rect.height as f32 / 2.0),
        )
    };
    let [top_left, top_right, bottom_right, bottom_left] = corners.map(clamp);
    rows(rect, |row| {
        let from_top = row as f32 + 0.5;
        let from_bottom = (rect.height - row) as f32 - 0.5;
        let left = inset(top_left, from_top).max(inset(bottom_left, from_bottom));
        let right = inset(top_right, from_top).max(inset(bottom_right, from_bottom));
        (left, right)
    })
}

fn inset((rx, ry): (f32, f32), from_edge: f32) -> i32 {
    if rx <= 0.0 || ry <= 0.0 || from_edge >= ry {
        return 0;
    }
    let dy = (ry - from_edge) / ry;
    (rx * (1.0 - (1.0 - dy * dy).sqrt())).round() as i32
}

fn arrow(rect: Rect, tip: Tip) -> Vec<Rect> {
    match tip {
        Tip::Up | Tip::Down => rows(rect, |row| {
            let step = match tip {
                Tip::Up => row,
                _ => rect.height - 1 - row,
            };
            let span = rect.width as f32 * (step + 1) as f32 / rect.height as f32;
            let side = ((rect.width as f32 - span) / 2.0).round() as i32;
            (side, side)
        }),
        Tip::Left | Tip::Right => {
            let turned = Rect {
                x: rect.y,
                y: rect.x,
                width: rect.height,
                height: rect.width,
            };
            let tip = match tip {
                Tip::Left => Tip::Up,
                _ => Tip::Down,
            };
            arrow(turned, tip)
                .into_iter()
                .map(|strip| Rect {
                    x: strip.y,
                    y: strip.x,
                    width: strip.height,
                    height: strip.width,
                })
                .collect()
        }
    }
}

fn rows(rect: Rect, insets: impl Fn(i32) -> (i32, i32)) -> Vec<Rect> {
    let mut strips: Vec<Rect> = Vec::new();
    let mut previous = None;
    for row in 0..rect.height {
        let (left, right) = insets(row);
        let width = rect.width - left - right;
        if width <= 0 {
            previous = None;
            continue;
        }
        if previous == Some((left, right))
            && let Some(last) = strips.last_mut()
        {
            last.height += 1;
            continue;
        }
        strips.push(Rect {
            x: rect.x + left,
            y: rect.y + row,
            width,
            height: 1,
        });
        previous = Some((left, right));
    }
    strips
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;
    use gtk4::{gdk, graphene, gsk};

    use super::{Rect, SQUARE, Tip, arrow, outline, rounded};

    const CARD: Rect = Rect {
        x: 10,
        y: 20,
        width: 200,
        height: 100,
    };

    fn area(strips: &[Rect]) -> i32 {
        strips.iter().map(|strip| strip.width * strip.height).sum()
    }

    fn covers(strips: &[Rect], x: i32, y: i32) -> bool {
        strips
            .iter()
            .any(|s| (s.x..s.x + s.width).contains(&x) && (s.y..s.y + s.height).contains(&y))
    }

    #[test]
    fn square_corners_are_one_rectangle() {
        assert_eq!(rounded(CARD, SQUARE), [CARD]);
    }

    #[test]
    fn a_rounded_corner_leaves_its_outer_pixel_clear_and_its_edges_whole() {
        let strips = rounded(CARD, [(12.0, 12.0); 4]);

        for (x, y) in [(10, 20), (209, 20), (10, 119), (209, 119)] {
            assert!(
                !covers(&strips, x, y),
                "the corner pixel at {x},{y} is blurred"
            );
        }
        for (x, y) in [(110, 20), (110, 119), (10, 70), (209, 70)] {
            assert!(
                covers(&strips, x, y),
                "the edge pixel at {x},{y} is not blurred"
            );
        }
    }

    #[test]
    fn a_rounded_region_is_symmetric_and_runs_merge() {
        let strips = rounded(CARD, [(12.0, 12.0); 4]);
        let mirrored: Vec<Rect> = strips
            .iter()
            .rev()
            .map(|s| Rect {
                y: CARD.y + CARD.height - (s.y - CARD.y) - s.height,
                ..*s
            })
            .collect();

        assert_eq!(strips, mirrored);
        assert!(strips.len() <= 25, "{} strips", strips.len());
        assert!(
            strips
                .iter()
                .any(|s| s.width == CARD.width && s.height >= CARD.height - 24)
        );
    }

    #[test]
    fn the_rounded_area_is_the_rectangle_less_four_corners() {
        let radius = 12.0_f32;
        let corners = 4.0 * (radius * radius - std::f32::consts::PI * radius * radius / 4.0);
        let expected = (CARD.width * CARD.height) as f32 - corners;

        let actual = area(&rounded(CARD, [(radius, radius); 4])) as f32;

        assert!(
            (actual - expected).abs() < 20.0,
            "{actual} against {expected}"
        );
    }

    #[test]
    fn a_radius_larger_than_the_rectangle_is_clamped_to_a_pill() {
        let strips = rounded(CARD, [(500.0, 500.0); 4]);

        assert!(covers(&strips, 110, 70));
        assert!(!covers(&strips, 10, 20));
        assert!(strips.iter().all(|s| s.width > 0 && s.height > 0));
    }

    #[test]
    fn corners_are_independent() {
        let strips = rounded(CARD, [(12.0, 12.0), (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)]);

        assert!(!covers(&strips, 10, 20));
        assert!(covers(&strips, 209, 20));
        assert!(covers(&strips, 10, 119));
        assert!(covers(&strips, 209, 119));
    }

    #[test]
    fn the_outline_is_the_rounded_clip_matching_the_widget_through_any_wrapping() {
        let rounded_clip = |width: f32, height: f32, radius: f32| {
            let bounds = graphene::Rect::new(0.0, 0.0, width, height);
            let corner = graphene::Size::new(radius, radius);
            let fill = gsk::ColorNode::new(&gdk::RGBA::RED, &bounds);
            gsk::RoundedClipNode::new(
                &fill,
                &gsk::RoundedRect::new(bounds, corner, corner, corner, corner),
            )
        };
        let button = rounded_clip(20.0, 20.0, 4.0);
        let card = gsk::OpacityNode::new(rounded_clip(200.0, 100.0, 14.0), 0.5);
        let tree = gsk::ContainerNode::new(&[button.upcast(), card.upcast()]);

        assert_eq!(
            outline(tree.upcast_ref(), 200.0, 100.0),
            Some([(14.0, 14.0); 4])
        );

        let flat = gsk::ColorNode::new(
            &gdk::RGBA::RED,
            &graphene::Rect::new(0.0, 0.0, 200.0, 100.0),
        );
        assert_eq!(outline(flat.upcast_ref(), 200.0, 100.0), None);
    }

    #[test]
    fn an_arrow_narrows_toward_its_tip_on_every_side() {
        let flat = Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 10,
        };
        let tall = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 20,
        };

        let up = arrow(flat, Tip::Up);
        assert!(covers(&up, 10, 0) && !covers(&up, 0, 0) && covers(&up, 0, 9));

        let down = arrow(flat, Tip::Down);
        assert!(covers(&down, 10, 9) && !covers(&down, 0, 9) && covers(&down, 0, 0));

        let left = arrow(tall, Tip::Left);
        assert!(covers(&left, 0, 10) && !covers(&left, 0, 0) && covers(&left, 9, 0));

        let right = arrow(tall, Tip::Right);
        assert!(covers(&right, 9, 10) && !covers(&right, 9, 0) && covers(&right, 0, 0));
    }

    #[test]
    fn an_arrow_is_about_half_its_box() {
        let flat = Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 10,
        };
        for tip in [Tip::Up, Tip::Down] {
            let covered = area(&arrow(flat, tip));
            assert!((90..=110).contains(&covered), "{tip:?} covers {covered}");
        }
    }
}
