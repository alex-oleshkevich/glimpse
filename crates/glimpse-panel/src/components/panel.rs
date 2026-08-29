use adw::gdk;
use glimpse_config::Position;
use glimpse_ipc::Client;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};
use std::collections::HashMap;

use crate::applet::runtime::AppletHandle;
use crate::applets;

pub struct Panel {
    window: gtk::Window,
    bar: glimpse_widgets::Panel,
    applets: Vec<Slot>,
}

struct Slot {
    zone: Zone,
    name: String,
    handle: Option<AppletHandle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Zone {
    Start,
    Center,
    End,
}

#[derive(Debug)]
pub struct Config {
    pub position: Position,
    pub size: u32,
    pub monitor: gdk::Monitor,
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
    pub client: Option<Client>,
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

#[derive(Debug)]
pub enum Input {
    Configure(Config),
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
        let mut model = Panel {
            window: window.clone(),
            bar: widgets.bar.clone(),
            applets: Vec::new(),
        };

        model.apply(&config);
        window.present();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::Configure(config) => self.apply(&config),
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
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            self.window.set_anchor(edge, anchors.contains(&edge));
        }
        self.bar.set_orientation(orientation);
        self.bar.set_thickness(config.size);
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
        let Some(client) = config.client.as_ref() else {
            return;
        };

        let desired: Vec<(Zone, &String)> = config
            .zones()
            .into_iter()
            .flat_map(|(zone, names)| names.iter().map(move |name| (zone, name)))
            .collect();

        if self
            .applets
            .iter()
            .map(|slot| (slot.zone, &slot.name))
            .eq(desired.iter().copied())
        {
            for handle in self.applets.iter().filter_map(|slot| slot.handle.as_ref()) {
                handle.group.set_orientation(orientation);
            }
            return;
        }

        self.bar.clear_start();
        self.bar.clear_center();
        self.bar.clear_end();

        let mut existing: HashMap<(Zone, String), Slot> = self
            .applets
            .drain(..)
            .map(|slot| ((slot.zone, slot.name.clone()), slot))
            .collect();

        let mut next = Vec::with_capacity(desired.len());
        for (zone, name) in desired {
            next.push(existing.remove(&(zone, name.clone())).unwrap_or_else(|| {
                Slot {
                    zone,
                    name: name.clone(),
                    handle: applets::resolve(name)
                        .map(|build| AppletHandle::launch(name.clone(), client.clone(), build)),
                }
            }));
        }

        for (zone, name) in existing
            .into_iter()
            .filter_map(|(key, slot)| slot.handle.is_some().then_some(key))
        {
            tracing::debug!(applet = name, ?zone, "applet removed");
        }

        for slot in &next {
            let Some(handle) = slot.handle.as_ref() else {
                continue;
            };
            handle.group.set_orientation(orientation);
            match slot.zone {
                Zone::Start => self.bar.append_to_start(&handle.group),
                Zone::Center => self.bar.append_to_center(&handle.group),
                Zone::End => self.bar.append_to_end(&handle.group),
            }
        }
        self.applets = next;
    }
}
