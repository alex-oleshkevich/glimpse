use adw::gdk::{self, prelude::*};
use futures_util::StreamExt;
use gtk4::prelude::{GtkWindowExt, WidgetExt};
use std::{collections::HashMap, path::PathBuf};

use glimpse_config::{Config, watch_config, watch_theme};
use glimpse_ipc::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
};
use tokio::task::JoinHandle;

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
    ThemeChanged,
}

pub struct App {
    config: Config,
    panels: Vec<PanelState>,
    theme_watch: JoinHandle<()>,
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
        let theme_watch = spawn_theme_watch(&init.config.appearance.theme, sender.clone());
        spawn_config_watch(init.config_path, init.config.clone(), sender);

        let model = App {
            config: init.config,
            panels: Default::default(),
            theme_watch,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppInput::ConfigChanged(config) => {
                if config.appearance.theme != self.config.appearance.theme {
                    self.theme_watch.abort();
                    self.theme_watch = spawn_theme_watch(&config.appearance.theme, sender);
                }
                self.config = config;
            }
            AppInput::MonitorsChanged => {}
            AppInput::ThemeChanged => tracing::debug!("theme changed"),
        }
        reconcile_panels(&mut self.panels, &self.config);
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

fn spawn_theme_watch(theme: &str, sender: ComponentSender<App>) -> JoinHandle<()> {
    let themes = watch_theme(theme);
    relm4::spawn(async move {
        let mut themes = Box::pin(themes);
        while themes.next().await.is_some() {
            sender.input(AppInput::ThemeChanged);
        }
    })
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
}

struct PanelState {
    pub key: Key,
    pub controller: Controller<components::panel::Panel>,
}

fn reconcile_panels(panels: &mut Vec<PanelState>, config: &Config) {
    tracing::debug!("reconciling panels");
    let mut existing: HashMap<Key, PanelState> = panels
        .drain(..)
        .map(|state| (state.key.clone(), state))
        .collect();

    let monitors = list_gdk_monitors();
    for (index, cfg) in config.panels.iter().enumerate() {
        for monitor in &monitors {
            let Some(connector) = monitor.connector().map(String::from) else {
                tracing::debug!("skipping monitor without a connector name");
                continue;
            };
            if cfg
                .monitor
                .as_deref()
                .is_some_and(|target| target != connector)
            {
                continue;
            }

            let key = Key {
                index,
                monitor: connector,
            };
            let panel_cfg = panel::Config {
                position: cfg.position,
                size: cfg.size,
                monitor: monitor.clone(),
            };
            let state = match existing.remove(&key) {
                Some(state) => {
                    state.controller.emit(panel::Input::Configure(panel_cfg));
                    state
                }
                None => PanelState {
                    key,
                    controller: panel::Panel::builder().launch(panel_cfg).detach(),
                },
            };
            panels.push(state);
        }
    }

    for (key, state) in existing {
        state.controller.widget().destroy();
        tracing::debug!(index = key.index, monitor = %key.monitor, "panel removed");
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
