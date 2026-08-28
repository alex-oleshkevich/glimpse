use futures_util::StreamExt;
use gtk4::prelude::{GtkWindowExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use std::path::PathBuf;

use glimpse_config::{
    Config, WALLPAPER_STYLESHEET, stylesheet, user_stylesheet, watch_config, watch_theme,
};
use glimpse_ipc::Client;
use glimpse_widgets::Styles;
use relm4::{ComponentParts, ComponentSender, SimpleComponent};
use tokio::task::JoinHandle;

pub struct AppInit {
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub socket: PathBuf,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AppInput {
    ConfigChanged(Config),
    ThemeChanged,
}

pub struct App {
    config: Config,
    theme_watch: JoinHandle<()>,
    styles: Styles,
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

        let theme_watch = spawn_theme_watch(&init.config.appearance.theme, sender.clone());
        spawn_config_watch(init.config_path, init.config.clone(), sender);
        spawn_daemon_client(init.socket);

        let model = App {
            config: init.config,
            theme_watch,
            styles: Styles::install(),
        };
        model.reload_styles();

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppInput::ConfigChanged(config) => {
                let renamed = config.appearance.theme != self.config.appearance.theme;
                self.config = config;
                if renamed {
                    self.theme_watch.abort();
                    self.theme_watch = spawn_theme_watch(&self.config.appearance.theme, sender);
                    self.reload_styles();
                }
            }
            AppInput::ThemeChanged => self.reload_styles(),
        }
    }
}

impl App {
    fn reload_styles(&self) {
        let theme = stylesheet(&self.config.appearance.theme, WALLPAPER_STYLESHEET);
        self.styles
            .load(theme.as_deref(), user_stylesheet().as_deref());
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
