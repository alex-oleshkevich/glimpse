use adw::gdk::{self, prelude::*};
use futures_util::StreamExt;
use gtk4::prelude::{GtkWindowExt, WidgetExt};
use std::{collections::HashMap, path::PathBuf};

use glimpse_config::{Config, Position, watch_config};
use glimpse_ipc::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
};

use crate::components::{self, panel};

pub struct AppInit {
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub socket: PathBuf,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AppInput {
    ConfigChanged(Config),
    MonitorsChanged,
}

pub struct App {
    config: Config,
    panels: Vec<PanelState>,
}

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = AppInit;
    type Input = AppInput;
    type Output = ();

    view! {
        adw::ApplicationWindow {
            set_visible: false,
            set_decorated: false,
            set_deletable: false,
            set_resizable: false,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        tracing::info!("initializing app");
        spawn_daemon_client(init.socket);
        watch_monitors(sender.clone());
        spawn_config_watch(init.config_path, init.config.clone(), sender);

        let model = App {
            config: init.config,
            panels: Default::default(),
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppInput::ConfigChanged(config) => {
                self.reconcile_panels(&config, sender);
                self.config = config;
            }
            AppInput::MonitorsChanged => {
                self.reconcile_panels(&self.config.clone(), sender);
            }
        }
    }
}

fn spawn_daemon_client(socket: PathBuf) {
    relm4::spawn(async move {
        let client = Client::open(&socket).await;
        let mut states = client.watch_state();
        while states.changed().await.is_ok() {
            tracing::debug!(state = ?*states.borrow_and_update(), "daemon connection");
        }
    });
}

fn spawn_config_watch(path: Option<PathBuf>, current: Config, sender: ComponentSender<App>) {
    relm4::spawn(async move {
        let mut configs = Box::pin(watch_config(path, current));
        while let Some(config) = configs.next().await {
            sender.input(AppInput::ConfigChanged(config));
        }
    });
}

fn watch_monitors(sender: ComponentSender<App>) {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let monitor_sender = sender.input_sender().clone();
    let _ = monitor_sender.send(AppInput::MonitorsChanged);
    display.monitors().connect_items_changed(move |_, _, _, _| {
        let _ = monitor_sender.send(AppInput::MonitorsChanged);
    });
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    pub index: usize,
    pub monitor: String,
    pub position: Position,
}

struct PanelState {
    pub key: Key,
    pub controller: Controller<components::panel::Panel>,
}

impl App {
    fn reconcile_panels(&mut self, config: &Config, _sender: ComponentSender<App>) {
        tracing::debug!("reconciling panels");
        let mut existing: HashMap<Key, PanelState> = self
            .panels
            .drain(..)
            .map(|state| (state.key.clone(), state))
            .collect();

        let monitors = list_gdk_monitors();
        let mut new_panels: Vec<PanelState> = Vec::new();
        for (index, cfg) in config.panels.iter().enumerate() {
            for monitor in &monitors {
                let connector = monitor_connector(monitor);
                if let Some(target) = cfg.monitor.as_deref()
                    && connector.as_deref() != Some(target)
                {
                    continue;
                }

                let key = Key {
                    index,
                    position: cfg.position,
                    monitor: connector.clone().unwrap_or_default(),
                };

                let panel_cfg = panel::Config {
                    position: cfg.position,
                    size: cfg.size,
                };
                let state = match existing.remove(&key) {
                    Some(state) => {
                        state.controller.emit(panel::Input::Configure(panel_cfg));
                        state
                    }
                    None => build_panel(key, panel_cfg, monitor.clone()),
                };
                new_panels.push(state);
            }
        }
        self.panels = new_panels;
        for (key, state) in existing.drain() {
            state.controller.widget().destroy();
            tracing::debug!(?key.position, index=key.index,monitor=%key.monitor, "panel removed");
        }
    }
}

fn list_gdk_monitors() -> Vec<gdk::Monitor> {
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };

    let model = display.monitors();
    (0..model.n_items())
        .filter_map(|i| model.item(i).and_downcast::<gdk::Monitor>())
        .collect()
}

fn monitor_connector(monitor: &gdk::Monitor) -> Option<String> {
    monitor.connector().map(|s| s.to_string())
}

fn build_panel(key: Key, config: panel::Config, monitor: gdk::Monitor) -> PanelState {
    let controller = panel::Panel::builder()
        .launch(panel::Init {
            config,
            monitor: Some(monitor),
        })
        .detach();
    PanelState { key, controller }
}
