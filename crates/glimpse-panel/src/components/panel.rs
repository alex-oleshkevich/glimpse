use adw::gdk;
use glimpse_config::{Applet as AppletConfig, Margin, Position, resolve_applet};
use glimpse_widgets::blur;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::mem::Discriminant;

use std::rc::Rc;

use crate::applet::catcher::Catcher;
use crate::applet::runtime::{AppletHandle, SlotInfo};
use crate::applets;

pub struct Panel {
    window: gtk::Window,
    bar: glimpse_widgets::Panel,
    applets: Vec<Slot>,
    catcher: Rc<Catcher>,
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
}

impl fmt::Debug for Input {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Input::Configure(_) => "Configure(..)",
            Input::ClosePopover => "ClosePopover",
        })
    }
}

#[relm4::component(pub)]
impl SimpleComponent for Panel {
    type Init = Config;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Window {
            #[name = "bar"]
            glimpse_widgets::Panel {}
        }
    }

    fn init(
        config: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.init_layer_shell();
        root.set_namespace(Some("glimpse-panel"));
        root.set_layer(Layer::Top);
        root.set_keyboard_mode(KeyboardMode::None);
        root.auto_exclusive_zone_enable();

        let window = root.clone();
        let widgets = view_output!();
        let bar = widgets.bar.clone();
        let mut model = Panel {
            window: window.clone(),
            bar: bar.clone(),
            applets: Vec::new(),
            catcher: Catcher::new(Some(&config.monitor), config.position),
        };

        model.apply(&config);
        window.present();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::Configure(config) => self.apply(&config),
            Input::ClosePopover => self.catcher.close(),
        }
    }
}

impl Panel {
    fn apply(&mut self, config: &Config) {
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
        for (edge, margin) in [
            (Edge::Top, config.margin.top),
            (Edge::Bottom, config.margin.bottom),
            (Edge::Left, config.margin.left),
            (Edge::Right, config.margin.right),
        ] {
            self.window.set_anchor(edge, anchors.contains(&edge));
            self.window.set_margin(edge, pixels(margin));
        }
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
