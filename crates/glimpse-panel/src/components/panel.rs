use adw::gdk;
use glimpse_config::{Applet as AppletConfig, Position, Regional};
use glimpse_dbus::{notifications::NotificationsProviderHandle, weather::WeatherProviderHandle};
use glimpse_services::{
    AudioHandle, BluetoothHandle, CalendarHandle, CompositorHandle, HeartbeatHandle,
    KeyboardHandle, MprisHandle, NetworkHandle, TrayHandle,
};
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
use crate::applet::runtime::AppletHandle;
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

fn settle(slot: &Slot, config: &Config, orientation: gtk::Orientation) {
    let Some(handle) = slot.handle.as_ref() else {
        return;
    };
    handle.set_orientation(orientation);
    if let Some(applet) = applets::configured(&slot.name, &config.applets, &config.regional) {
        handle.configure(applet);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Zone {
    Start,
    Center,
    End,
}

pub struct Config {
    pub position: Position,
    pub size: u32,
    pub monitor: gdk::Monitor,
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
    pub applets: BTreeMap<String, AppletConfig>,
    pub regional: Regional,
    pub compositor: CompositorHandle,
    pub keyboard: KeyboardHandle,
    pub calendar: CalendarHandle,
    pub mpris: MprisHandle,
    pub heartbeat: HeartbeatHandle,
    pub tray: TrayHandle,
    pub bluetooth: BluetoothHandle,
    pub network: NetworkHandle,
    pub audio: AudioHandle,
    pub notifications: NotificationsProviderHandle,
    pub weather: WeatherProviderHandle,
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
        let mut model = Panel {
            window: window.clone(),
            bar: widgets.bar.clone(),
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
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            self.window.set_anchor(edge, anchors.contains(&edge));
        }
        self.bar.set_orientation(orientation);
        self.bar.set_thickness(config.size);
        self.catcher.reconfigure(&config.monitor, config.position);
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
        let desired: Vec<(
            Zone,
            &String,
            Option<Discriminant<glimpse_config::AppletKind>>,
        )> = config
            .zones()
            .into_iter()
            .flat_map(|(zone, names)| {
                names.iter().map(move |name| {
                    let kind = applets::configured(name, &config.applets, &config.regional)
                        .map(|applet| std::mem::discriminant(&applet.kind));
                    (zone, name, kind)
                })
            })
            .collect();

        if self
            .applets
            .iter()
            .map(|slot| (slot.zone, &slot.name, slot.kind))
            .eq(desired.iter().copied())
        {
            for slot in &self.applets {
                settle(slot, config, orientation);
            }
            return;
        }

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
        for (zone, name, kind) in desired {
            next.push(
                existing
                    .remove(&(zone, name.clone(), kind))
                    .unwrap_or_else(|| Slot {
                        zone,
                        name: name.clone(),
                        kind,
                        handle: applets::configured(name, &config.applets, &config.regional)
                            .and_then(|applet| {
                                let Some(build) = applets::build(
                                    &applet,
                                    &config.compositor,
                                    &config.keyboard,
                                    &config.calendar,
                                    &config.mpris,
                                    &config.heartbeat,
                                    &config.tray,
                                    &config.bluetooth,
                                    &config.network,
                                    &config.audio,
                                    &config.notifications,
                                    &config.weather,
                                ) else {
                                    tracing::debug!(
                                        applet = name,
                                        "applet is not implemented yet, skipping"
                                    );
                                    return None;
                                };
                                Some(AppletHandle::launch(
                                    name.clone(),
                                    connector.clone(),
                                    build,
                                    applet,
                                    Rc::clone(&self.catcher),
                                ))
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

        for slot in &next {
            settle(slot, config, orientation);
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
