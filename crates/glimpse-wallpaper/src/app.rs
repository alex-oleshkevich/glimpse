use futures_util::StreamExt;
use gtk4::prelude::{GtkWindowExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use std::path::PathBuf;

use glimpse_config::{Config, watch_config};
use glimpse_ipc::Client;
use relm4::{ComponentParts, ComponentSender, SimpleComponent};

pub struct AppInit {
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub socket: PathBuf,
}

#[derive(Debug)]
pub enum AppInput {
    ConfigChanged(Config),
}

pub struct App {
    config: Config,
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
        root.init_layer_shell();
        root.set_layer(gtk4_layer_shell::Layer::Background);
        root.set_namespace(Some("glimpse-wallpaper"));

        spawn_config_watch(init.config_path, init.config.clone(), sender);
        spawn_daemon_client(init.socket);

        let model = App {
            config: init.config,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            AppInput::ConfigChanged(config) => self.config = config,
        }
    }
}

fn spawn_daemon_client(socket: PathBuf) {
    relm4::spawn(async move {
        let client = Client::open(&socket);
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
