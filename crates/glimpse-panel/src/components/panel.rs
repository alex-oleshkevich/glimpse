use adw::{gdk, prelude::AnimationExt};
use glimpse_config::{Applet as AppletConfig, Margin, Position, resolve_applet};
use glimpse_widgets::blur;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, cairo, glib, prelude::*},
};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::mem::Discriminant;
use std::time::Duration;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use crate::applet::catcher::Catcher;
use crate::applet::runtime::{AppletHandle, SlotInfo};
use crate::applets;

mod presence;

use presence::{Hold, Presence, Step};

const DWELL: Duration = Duration::from_millis(150);
const EDGE_STRIP: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    Whole,
    Nothing,
    Edge(Position),
}

fn reach(surface: &gdk::Surface, reach: Reach) {
    let (width, height) = (surface.width(), surface.height());
    let region = match reach {
        Reach::Whole => None,
        Reach::Nothing => Some(cairo::Region::create()),
        Reach::Edge(position) => {
            let strip = match position {
                Position::Top => cairo::RectangleInt::new(0, 0, width, EDGE_STRIP),
                Position::Bottom => {
                    cairo::RectangleInt::new(0, height - EDGE_STRIP, width, EDGE_STRIP)
                }
                Position::Left => cairo::RectangleInt::new(0, 0, EDGE_STRIP, height),
                Position::Right => {
                    cairo::RectangleInt::new(width - EDGE_STRIP, 0, EDGE_STRIP, height)
                }
            };
            Some(cairo::Region::create_rectangle(&strip))
        }
    };
    surface.set_input_region(region.as_ref());
}

struct Reservation {
    window: gtk::Window,
    catcher: Rc<Catcher>,
    full: Cell<i32>,
}

impl Reservation {
    fn track(&self, fraction: f64) {
        let full = self.full.get();
        let zone = (fraction * f64::from(full)).round() as i32;
        self.window.set_exclusive_zone(zone);
        self.catcher.set_clearance(full - zone);
    }

    fn settle(&self, reserved: bool) {
        if reserved {
            self.window.auto_exclusive_zone_enable();
            self.catcher.set_clearance(0);
        } else {
            self.window.set_exclusive_zone(0);
            self.catcher.set_clearance(self.full.get());
        }
    }
}

pub struct Panel {
    window: gtk::Window,
    bar: glimpse_widgets::Panel,
    slide: glimpse_widgets::Slide,
    applets: Vec<Slot>,
    catcher: Rc<Catcher>,
    reservation: Rc<Reservation>,
    reserving: adw::TimedAnimation,
    presence: Presence,
    position: Position,
    margin: Margin,
    edge_reveal: bool,
    hide_delay: Duration,
    linger: u64,
    dwell: u64,
    reach: Rc<Cell<Reach>>,
    reported: Option<Output>,
}

struct Slot {
    zone: Zone,
    name: String,
    kind: Option<Discriminant<glimpse_config::AppletKind>>,
    handle: Option<AppletHandle>,
}

fn settle(slot: &Slot, config: &Config, orientation: gtk::Orientation, index: u32) {
    let Some(handle) = slot.handle.as_ref() else {
        return;
    };
    handle.set_slot(slot_info(config, slot.zone, index, &slot.name));
    handle.set_orientation(orientation);
    handle.set_placement(applets::exec::placement(
        config.position,
        config.size,
        config.monitor.connector().map(String::from),
        slot.zone,
    ));
    if let Some(applet) = resolve_applet(&slot.name, &config.applets) {
        handle.configure(applet);
    }
}

fn slot_info(config: &Config, zone: Zone, index: u32, name: &str) -> SlotInfo {
    SlotInfo {
        connector: config
            .monitor
            .connector()
            .map(String::from)
            .unwrap_or_default(),
        position: config.position,
        zone,
        index,
        name: name.to_owned(),
    }
}

fn indices(zones: impl Iterator<Item = Zone>) -> Vec<u32> {
    let mut counts: HashMap<Zone, u32> = HashMap::new();
    zones
        .map(|zone| {
            let count = counts.entry(zone).or_default();
            *count += 1;
            *count - 1
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Zone {
    Start,
    Center,
    End,
}

pub struct Config {
    pub position: Position,
    pub size: u32,
    pub margin: Margin,
    pub monitor: gdk::Monitor,
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
    pub attached: Vec<(Zone, String)>,
    pub applets: BTreeMap<String, AppletConfig>,
    pub blur: bool,
    pub pinned: bool,
    pub held: bool,
    pub edge_reveal: bool,
    pub hide_delay: Duration,
    pub handles: crate::services::Handles,
    pub dialog: relm4::Sender<crate::app::AppInput>,
}

impl Config {
    fn zones(&self) -> [(Zone, &[String]); 3] {
        [
            (Zone::Start, &self.left),
            (Zone::Center, &self.center),
            (Zone::End, &self.right),
        ]
    }
}

fn desired(
    zones: [(Zone, &[String]); 3],
    applets: &BTreeMap<String, AppletConfig>,
    attached: &[(Zone, String)],
) -> Vec<(
    Zone,
    String,
    Option<Discriminant<glimpse_config::AppletKind>>,
)> {
    let mut desired = Vec::new();
    let configured_ids: Vec<_> = zones
        .iter()
        .flat_map(|(_, names)| names.iter())
        .filter_map(|name| resolve_applet(name, applets))
        .filter_map(|applet| match applet.kind {
            glimpse_config::AppletKind::Exec(exec) => Some(exec.applet),
            _ => None,
        })
        .collect();
    for (zone, names) in zones {
        for name in names {
            let applet = resolve_applet(name, applets);
            let kind = applet.map(|applet| std::mem::discriminant(&applet.kind));
            desired.push((zone, name.clone(), kind));
        }
        for (_, id) in attached.iter().filter(|(target, _)| *target == zone) {
            if !configured_ids.contains(id) {
                let kind =
                    resolve_applet(id, applets).map(|applet| std::mem::discriminant(&applet.kind));
                desired.push((zone, id.clone(), kind));
            }
        }
    }
    desired
}

#[allow(
    clippy::large_enum_variant,
    reason = "Configure carries the whole panel config, and both variants are sent rarely; boxing               it would allocate on every reconfigure to satisfy a size heuristic"
)]
pub enum Input {
    Configure(Config),
    ClosePopover,
    Enter,
    Leave,
    Dwell(u64),
    Expire(u64),
    Popover(bool),
    Slid,
    Layout,
}

impl fmt::Debug for Input {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Input::Configure(_) => formatter.write_str("Configure(..)"),
            Input::ClosePopover => formatter.write_str("ClosePopover"),
            Input::Enter => formatter.write_str("Enter"),
            Input::Leave => formatter.write_str("Leave"),
            Input::Dwell(serial) => write!(formatter, "Dwell({serial})"),
            Input::Expire(serial) => write!(formatter, "Expire({serial})"),
            Input::Popover(open) => write!(formatter, "Popover({open})"),
            Input::Slid => formatter.write_str("Slid"),
            Input::Layout => formatter.write_str("Layout"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Output {
    pub pinned: bool,
    pub visible: bool,
}

#[relm4::component(pub)]
impl SimpleComponent for Panel {
    type Init = Config;
    type Input = Input;
    type Output = Output;

    view! {
        root = gtk::Window {
            add_css_class: "panel-window",

            #[name = "slide"]
            glimpse_widgets::Slide {
                #[name = "bar"]
                #[wrap(Some)]
                set_child = &glimpse_widgets::Panel {},
            }
        }
    }

    fn init(
        config: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.init_layer_shell();
        root.set_namespace(Some("glimpse-panel"));
        root.set_layer(Layer::Top);
        root.set_keyboard_mode(KeyboardMode::None);
        root.set_decorated(false);

        let window = root.clone();
        let widgets = view_output!();
        let catcher = Catcher::new(Some(&config.monitor), config.position);
        let input = sender.input_sender().clone();
        catcher.connect_open(move |open| {
            let _ = input.send(Input::Popover(open));
        });

        let motion = gtk::EventControllerMotion::new();
        let input = sender.input_sender().clone();
        motion.connect_enter(move |_, _, _| {
            let _ = input.send(Input::Enter);
        });
        let input = sender.input_sender().clone();
        motion.connect_leave(move |_| {
            let _ = input.send(Input::Leave);
        });
        window.add_controller(motion);

        let mut presence = Presence::new(config.pinned);
        let _ = presence.hold(Hold::Command, config.held);
        let reservation = Rc::new(Reservation {
            window: window.clone(),
            catcher: catcher.clone(),
            full: Cell::new(0),
        });
        let reserved = if presence.pinned() { 1.0 } else { 0.0 };
        let reserving = adw::TimedAnimation::new(
            &window,
            reserved,
            reserved,
            widgets.slide.duration(),
            adw::CallbackAnimationTarget::new({
                let reservation = Rc::downgrade(&reservation);
                move |fraction| {
                    if let Some(reservation) = reservation.upgrade() {
                        reservation.track(fraction);
                    }
                }
            }),
        );
        reserving.connect_done({
            let reservation = Rc::downgrade(&reservation);
            move |animation| {
                if let Some(reservation) = reservation.upgrade() {
                    reservation.settle(animation.value_to() >= 1.0);
                }
            }
        });
        reservation.settle(presence.pinned());
        widgets.slide.jump(presence.visible());

        let mut model = Panel {
            window: window.clone(),
            bar: widgets.bar.clone(),
            slide: widgets.slide.clone(),
            applets: Vec::new(),
            catcher,
            reservation,
            reserving,
            presence,
            position: config.position,
            margin: config.margin,
            edge_reveal: config.edge_reveal,
            hide_delay: config.hide_delay,
            linger: 0,
            dwell: 0,
            reach: Rc::new(Cell::new(Reach::Whole)),
            reported: None,
        };

        model.apply(&config);
        let realized = model.reach.clone();
        let input = sender.input_sender().clone();
        window.connect_realize(move |window| {
            let Some(surface) = window.surface() else {
                return;
            };
            let layout = realized.clone();
            let input = input.clone();
            surface.connect_layout(move |surface, _, _| {
                reach(surface, layout.get());
                let _ = input.send(Input::Layout);
            });
        });
        window.present();
        model.region();
        model.refresh();
        model.report(&sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        let steps = match message {
            Input::Configure(config) => {
                self.apply(&config);
                self.region();
                self.refresh();
                let mut steps = self.presence.pin(config.pinned);
                steps.extend(self.presence.hold(Hold::Command, config.held));
                steps
            }
            Input::ClosePopover => {
                self.catcher.close();
                Vec::new()
            }
            Input::Enter if self.presence.visible() => self.presence.hold(Hold::Pointer, true),
            Input::Enter => {
                if self.edge_reveal {
                    self.dwell += 1;
                    let serial = self.dwell;
                    let input = sender.input_sender().clone();
                    glib::timeout_add_local_once(DWELL, move || {
                        let _ = input.send(Input::Dwell(serial));
                    });
                }
                Vec::new()
            }
            Input::Leave => {
                self.dwell += 1;
                self.presence.hold(Hold::Pointer, false)
            }
            Input::Dwell(serial) if serial == self.dwell => self.presence.hold(Hold::Pointer, true),
            Input::Expire(serial) if serial == self.linger => self.presence.expire(),
            Input::Dwell(_) | Input::Expire(_) => Vec::new(),
            Input::Popover(open) => self.presence.hold(Hold::Popover, open),
            Input::Slid => self.presence.slid(),
            Input::Layout => {
                self.refresh();
                Vec::new()
            }
        };
        self.run(steps, &sender);
    }
}

impl Panel {
    fn run(&mut self, steps: Vec<Step>, sender: &ComponentSender<Self>) {
        for step in steps {
            match step {
                Step::Reserve => self
                    .slide
                    .drive(&self.reserving, self.reserving.value(), true),
                Step::Release => {
                    self.catcher.close();
                    self.slide
                        .drive(&self.reserving, self.reserving.value(), false);
                }
                Step::SlideIn => self.slide.reveal(true, || {}),
                Step::SlideOut => {
                    let input = sender.input_sender().clone();
                    self.slide.reveal(false, move || {
                        let _ = input.send(Input::Slid);
                    });
                }
                Step::Linger => {
                    self.linger += 1;
                    let serial = self.linger;
                    let input = sender.input_sender().clone();
                    glib::timeout_add_local_once(self.hide_delay, move || {
                        let _ = input.send(Input::Expire(serial));
                    });
                }
                Step::Settle => self.linger += 1,
                Step::Region => self.region(),
            }
        }
        self.report(sender);
    }

    fn report(&mut self, sender: &ComponentSender<Self>) {
        let state = Output {
            pinned: self.presence.pinned(),
            visible: self.presence.visible(),
        };
        if self.reported.replace(state) != Some(state) {
            let _ = sender.output(state);
        }
    }

    fn refresh(&self) {
        let full = self.full_zone();
        self.reservation.full.set(full);
        if self.reserving.state() != adw::AnimationState::Playing && !self.presence.pinned() {
            self.catcher.set_clearance(full);
        }
    }

    fn full_zone(&self) -> i32 {
        match self.position {
            Position::Top => self.window.height() + pixels(self.margin.bottom),
            Position::Bottom => self.window.height() + pixels(self.margin.top),
            Position::Left => self.window.width() + pixels(self.margin.right),
            Position::Right => self.window.width() + pixels(self.margin.left),
        }
    }

    fn region(&self) {
        let shown = self.presence.visible() || self.slide.shown();
        let wanted = match (shown, self.edge_reveal) {
            (true, _) => Reach::Whole,
            (false, false) => Reach::Nothing,
            (false, true) => Reach::Edge(self.position),
        };
        self.reach.set(wanted);
        if let Some(surface) = self.window.surface() {
            reach(&surface, wanted);
            self.window.queue_draw();
        }
    }

    fn apply(&mut self, config: &Config) {
        self.position = config.position;
        self.margin = config.margin;
        self.edge_reveal = config.edge_reveal;
        self.hide_delay = config.hide_delay;
        let (anchors, orientation) = match config.position {
            Position::Top => (
                [Edge::Top, Edge::Left, Edge::Right],
                gtk::Orientation::Horizontal,
            ),
            Position::Bottom => (
                [Edge::Bottom, Edge::Left, Edge::Right],
                gtk::Orientation::Horizontal,
            ),
            Position::Left => (
                [Edge::Left, Edge::Top, Edge::Bottom],
                gtk::Orientation::Vertical,
            ),
            Position::Right => (
                [Edge::Right, Edge::Top, Edge::Bottom],
                gtk::Orientation::Vertical,
            ),
        };

        if self.window.monitor().as_ref() != Some(&config.monitor) {
            self.window.set_monitor(Some(&config.monitor));
        }
        let anchored = crate::applet::catcher::bar_edge(config.position);
        for (edge, margin) in [
            (Edge::Top, config.margin.top),
            (Edge::Bottom, config.margin.bottom),
            (Edge::Left, config.margin.left),
            (Edge::Right, config.margin.right),
        ] {
            self.window.set_anchor(edge, anchors.contains(&edge));
            self.window
                .set_margin(edge, if edge == anchored { 0 } else { pixels(margin) });
        }
        let inner = |edge: Edge, margin: u32| if edge == anchored { pixels(margin) } else { 0 };
        self.bar.set_margin_top(inner(Edge::Top, config.margin.top));
        self.bar
            .set_margin_bottom(inner(Edge::Bottom, config.margin.bottom));
        self.bar
            .set_margin_start(inner(Edge::Left, config.margin.left));
        self.bar
            .set_margin_end(inner(Edge::Right, config.margin.right));
        self.slide.set_edge(match config.position {
            Position::Top => gtk::PositionType::Top,
            Position::Bottom => gtk::PositionType::Bottom,
            Position::Left => gtk::PositionType::Left,
            Position::Right => gtk::PositionType::Right,
        });
        self.bar.set_orientation(orientation);
        self.bar.set_thickness(config.size);
        let inset = match orientation {
            gtk::Orientation::Horizontal => config.margin.left,
            _ => config.margin.top,
        };
        self.catcher
            .reconfigure(&config.monitor, config.position, pixels(inset), config.blur);
        blur::apply(&self.window, config.blur);
        self.reconcile_applets(config, orientation);

        tracing::debug!(
            position = ?config.position,
            size = config.size,
            monitor = ?config.monitor.connector(),
            applets = self.applets.iter().filter(|slot| slot.handle.is_some()).count(),
            "panel configured"
        );
    }

    fn reconcile_applets(&mut self, config: &Config, orientation: gtk::Orientation) {
        let desired = desired(config.zones(), &config.applets, &config.attached);

        if self
            .applets
            .iter()
            .map(|slot| (slot.zone, &slot.name, &slot.kind))
            .eq(desired.iter().map(|(zone, name, kind)| (*zone, name, kind)))
        {
            let indices = indices(self.applets.iter().map(|slot| slot.zone));
            for (slot, index) in self.applets.iter().zip(indices) {
                settle(slot, config, orientation, index);
            }
            return;
        }
        let indices = indices(desired.iter().map(|(zone, _, _)| *zone));

        let connector = config.monitor.connector().map(String::from);

        self.bar.clear_start();
        self.bar.clear_center();
        self.bar.clear_end();

        let mut existing: HashMap<
            (
                Zone,
                String,
                Option<Discriminant<glimpse_config::AppletKind>>,
            ),
            Slot,
        > = self
            .applets
            .drain(..)
            .map(|slot| ((slot.zone, slot.name.clone(), slot.kind), slot))
            .collect();

        let mut next = Vec::with_capacity(desired.len());
        for ((zone, name, kind), index) in desired.into_iter().zip(indices.iter().copied()) {
            next.push(
                existing
                    .remove(&(zone, name.clone(), kind))
                    .unwrap_or_else(|| Slot {
                        zone,
                        name: name.clone(),
                        kind,
                        handle: resolve_applet(&name, &config.applets).map(|applet| {
                            let build = applets::build(
                                &applet,
                                &config.handles,
                                crate::applets::exec::placement(
                                    config.position,
                                    config.size,
                                    connector.clone(),
                                    zone,
                                ),
                                &config.dialog,
                            );
                            AppletHandle::launch(
                                name.clone(),
                                connector.clone(),
                                build,
                                applet,
                                Rc::clone(&self.catcher),
                                slot_info(config, zone, index, &name),
                                Arc::clone(&config.handles.osd_quiet),
                            )
                        }),
                    }),
            );
        }

        for (zone, name, _) in existing
            .into_iter()
            .filter_map(|(key, slot)| slot.handle.is_some().then_some(key))
        {
            tracing::debug!(applet = name, ?zone, "applet removed");
        }

        for (slot, index) in next.iter().zip(indices) {
            settle(slot, config, orientation, index);
            let Some(handle) = slot.handle.as_ref() else {
                continue;
            };
            match slot.zone {
                Zone::Start => self.bar.append_to_start(&handle.widget),
                Zone::Center => self.bar.append_to_center(&handle.widget),
                Zone::End => self.bar.append_to_end(&handle.widget),
            }
        }
        self.applets = next;
    }
}

fn pixels(margin: u32) -> i32 {
    i32::try_from(margin).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests;
