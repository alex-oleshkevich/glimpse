use adw::gdk;
use futures_util::StreamExt;
use glimpse_config::{
    Config, DARK_STYLESHEET, WALLPAPER_STYLESHEET, stylesheet, user_dark_stylesheet,
    user_stylesheet, watch_config, watch_theme,
};
use glimpse_widgets::{Sheets, Styles};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::prelude::*,
};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::task::JoinHandle;

use crate::resolve::{self, Key, MissingWarned, Role};
use crate::surface;

pub struct AppInit {
    pub config: Config,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AppInput {
    ConfigChanged(Config),
    ThemeChanged,
    MonitorsChanged,
    DarkFlipped,
}

struct SurfaceState {
    key: Key,
    controller: Controller<surface::Surface>,
}

pub struct App {
    config: Config,
    theme_watch: JoinHandle<()>,
    styles: Styles,
    surfaces: Vec<SurfaceState>,
    dark: bool,
    missing_warned: MissingWarned,
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
        watch_monitors(sender.clone());
        watch_scheme(sender.clone());
        let theme_watch = spawn_theme_watch(&init.config.appearance.theme, sender.clone());
        spawn_config_watch(init.config_path, init.config.clone(), sender);

        tracing::info!(
            "the backdrop surface is visible only where niri places it; add \
             `layer-rule {{ match namespace=\"^glimpse-backdrop$\"; place-within-backdrop true; }}` \
             to show it in the Overview"
        );

        let styles = Styles::install(color_scheme(init.config.appearance.color_scheme));
        let dark = adw::StyleManager::default().is_dark();
        let mut model = App {
            config: init.config,
            theme_watch,
            styles,
            surfaces: Vec::new(),
            dark,
            missing_warned: MissingWarned::default(),
        };
        model.reload_styles();
        reconcile_surfaces(
            &mut model.surfaces,
            &mut model.missing_warned,
            &model.config,
            model.dark,
        );
        model
            .styles
            .set_variant(&model.config.appearance.theme_variant);

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppInput::ConfigChanged(config) => {
                let renamed = config.appearance.theme != self.config.appearance.theme;
                glimpse_utils::report_language_change(
                    self.config.regional.language(),
                    config.regional.language(),
                );
                self.config = config;
                self.styles
                    .set_color_scheme(color_scheme(self.config.appearance.color_scheme));
                self.dark = adw::StyleManager::default().is_dark();
                if renamed {
                    self.theme_watch.abort();
                    self.theme_watch = spawn_theme_watch(&self.config.appearance.theme, sender);
                    self.reload_styles();
                }
            }
            AppInput::ThemeChanged => self.reload_styles(),
            AppInput::MonitorsChanged => {}
            AppInput::DarkFlipped => {
                self.dark = adw::StyleManager::default().is_dark();
            }
        }
        reconcile_surfaces(
            &mut self.surfaces,
            &mut self.missing_warned,
            &self.config,
            self.dark,
        );
        self.styles
            .set_variant(&self.config.appearance.theme_variant);
    }
}

fn color_scheme(scheme: glimpse_config::ColorScheme) -> adw::ColorScheme {
    match scheme {
        glimpse_config::ColorScheme::Light => adw::ColorScheme::ForceLight,
        glimpse_config::ColorScheme::Dark => adw::ColorScheme::ForceDark,
        glimpse_config::ColorScheme::Auto => adw::ColorScheme::Default,
    }
}

impl App {
    fn reload_styles(&self) {
        let appearance = &self.config.appearance;
        self.styles.load(&Sheets {
            theme: stylesheet(&appearance.theme, WALLPAPER_STYLESHEET),
            theme_dark: stylesheet(&appearance.theme, DARK_STYLESHEET),
            dropin: user_stylesheet(),
            dropin_dark: user_dark_stylesheet(),
        });
        self.styles.set_variant(&appearance.theme_variant);
    }
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
    let monitor_sender = sender.input_sender().clone();
    let _ = monitor_sender.send(AppInput::MonitorsChanged);
    glimpse_widgets::watch_monitors(move || {
        let _ = monitor_sender.send(AppInput::MonitorsChanged);
    });
}

fn watch_scheme(sender: ComponentSender<App>) {
    let scheme_sender = sender.input_sender().clone();
    adw::StyleManager::default().connect_dark_notify(move |_| {
        let _ = scheme_sender.send(AppInput::DarkFlipped);
    });
}

fn reconcile_surfaces(
    surfaces: &mut Vec<SurfaceState>,
    missing_warned: &mut MissingWarned,
    config: &Config,
    dark: bool,
) {
    let mut existing: HashMap<Key, SurfaceState> = surfaces
        .drain(..)
        .map(|state| (state.key.clone(), state))
        .collect();

    for monitor in list_gdk_monitors() {
        let Some(connector) = monitor.connector().map(String::from) else {
            tracing::debug!("skipping monitor without a connector name");
            continue;
        };

        for role in [Role::Wallpaper, Role::Backdrop] {
            let Some(intent) =
                resolve::intent(&config.wallpaper, &connector, role, dark, missing_warned)
            else {
                continue;
            };

            let key = Key {
                connector: connector.clone(),
                role,
            };
            let surface_config = surface::Config {
                monitor: monitor.clone(),
                role,
                intent,
            };
            let state = match existing.remove(&key) {
                Some(state) => {
                    state
                        .controller
                        .emit(surface::Input::Configure(surface_config));
                    state
                }
                None => SurfaceState {
                    key,
                    controller: surface::Surface::builder().launch(surface_config).detach(),
                },
            };
            surfaces.push(state);
        }
    }

    for (key, state) in existing {
        state.controller.widget().destroy();
        tracing::debug!(connector = %key.connector, role = ?key.role, "wallpaper surface removed");
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
