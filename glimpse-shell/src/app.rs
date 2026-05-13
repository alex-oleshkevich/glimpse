use std::collections::HashMap;

use crate::{
    agents::{bluetooth::BluetoothAgentRuntime, network::NetworkAgentRuntime},
    panels,
    prompts::{bluetooth as bluetooth_prompts, network as network_prompts},
    services::{
        framework::{Control, ServiceRuntime, Services},
        wayland_idle_inhibit::{
            self, NoopWaylandInhibitor, SHELL_EXTENSIONS, ShellExtensions, WaylandIdleInhibitor,
            gdk_backend::GdkWaylandInhibitor,
        },
    },
    theme::{self, ThemeState},
};
use adw::gdk::{self, prelude::DisplayExt, prelude::MonitorExt};
use gio::prelude::ListModelExt;
use glib::object::{Cast, CastNone};
use glimpse_core::{
    Config, ConfigEvent, DiscoveredApplets, PanelConfig, config::merge_applet_configs,
    services::theme::State as ThemeServiceState, watch_for_config_changes,
};
use gtk4::prelude::{GtkWindowExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct AppInit {
    pub config: Config,
    pub dbus: glimpse_core::dbus::Dbus,
}

#[derive(Debug)]
pub enum Input {
    ConfigChanged(Config),
    AppletDirsChanged(DiscoveredApplets),
    ThemeReload,
    ThemeChanged(ThemeServiceState),
    MonitorsChanged,
}

pub struct App {
    config: Config,
    discovered_applets: DiscoveredApplets,
    services: ServiceRuntime,
    theme: ThemeState,
    panels: Vec<PanelState>,
    network_prompt_host: Controller<network_prompts::PromptHost>,
    bluetooth_prompt_host: Controller<bluetooth_prompts::PromptHost>,
    network_agent_cancel: CancellationToken,
    bluetooth_agent_cancel: CancellationToken,
    prompt_fallback_parent: gtk4::Widget,
    wayland_swap_tx: tokio::sync::mpsc::Sender<Box<dyn WaylandIdleInhibitor + Send>>,
    wayland_installed: bool,
    wayland_host_key: Option<panels::PanelKey>,
}

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = AppInit;
    type Input = Input;
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
        root.set_namespace(Some("glimpse-shell"));
        root.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
        root.set_default_size(-1, -1);
        root.set_opacity(0.0);
        theme::apply_theme_mode(&root, &theme::DIALOG_THEME_MODE);

        let (config_tx, mut config_rx) = mpsc::channel(1);
        relm4::spawn(async move {
            watch_for_config_changes(config_tx).await;
        });

        let config_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            loop {
                match config_rx.recv().await {
                    Some(message) => match message {
                        ConfigEvent::Changed(config) => {
                            if config_sender.send(Input::ConfigChanged(config)).is_err() {
                                break;
                            }
                        }
                    },
                    None => break,
                }
            }
        });

        if let Some(display) = gdk::Display::default() {
            let monitor_sender = sender.input_sender().clone();
            let _ = monitor_sender.send(Input::MonitorsChanged);
            display.monitors().connect_items_changed(move |_, _, _, _| {
                let _ = monitor_sender.send(Input::MonitorsChanged);
            });
        }

        let theme = ThemeState::install(&init.config);

        let (theme_tx, mut theme_rx) = mpsc::channel::<()>(1);
        relm4::spawn(async move {
            theme::watch_user_themes(theme_tx).await;
        });

        let theme_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            while theme_rx.recv().await.is_some() {
                if theme_sender.send(Input::ThemeReload).is_err() {
                    break;
                }
            }
        });

        let system_dbus = init.dbus.system.clone();
        let (network_agent_runtime, network_agent) = NetworkAgentRuntime::new(system_dbus.clone());
        let network_agent_cancel = CancellationToken::new();
        {
            let cancel = network_agent_cancel.clone();
            relm4::spawn(async move {
                network_agent_runtime.run(cancel).await;
            });
        }

        let (bluetooth_agent_runtime, bluetooth_agent) = BluetoothAgentRuntime::new(system_dbus);
        let bluetooth_agent_cancel = CancellationToken::new();
        {
            let cancel = bluetooth_agent_cancel.clone();
            relm4::spawn(async move {
                bluetooth_agent_runtime.run(cancel).await;
            });
        }

        let wayland_swap_tx = spawn_idle_subsystem(init.dbus.session.clone());

        let services = ServiceRuntime::new(init.dbus);
        services.broadcast(Control::Start(init.config.clone()));
        spawn_theme_subscription(services.handles().theme, sender.input_sender().clone());

        let mut applet_watcher_rx = services.handles().applet_watcher.clone();
        let discovered_applets = applet_watcher_rx.borrow_and_update().clone();
        let applet_watcher_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            while applet_watcher_rx.changed().await.is_ok() {
                let discovered = applet_watcher_rx.borrow_and_update().clone();
                if applet_watcher_sender
                    .send(Input::AppletDirsChanged(discovered))
                    .is_err()
                {
                    break;
                }
            }
        });

        let prompt_fallback_parent: gtk4::Widget = root.clone().upcast();

        let network_prompt_host = network_prompts::PromptHost::builder()
            .launch(network_prompts::PromptHostInit {
                agent: network_agent,
                parent: prompt_fallback_parent.clone(),
                theme_mode: theme::DIALOG_THEME_MODE,
            })
            .detach();

        let bluetooth_prompt_host = bluetooth_prompts::PromptHost::builder()
            .launch(bluetooth_prompts::PromptHostInit {
                agent: bluetooth_agent,
                parent: prompt_fallback_parent.clone(),
                theme_mode: theme::DIALOG_THEME_MODE,
            })
            .detach();

        let widgets = view_output!();
        let model = App {
            config: init.config,
            discovered_applets,
            services,
            theme,
            panels: vec![],
            network_prompt_host,
            bluetooth_prompt_host,
            network_agent_cancel,
            bluetooth_agent_cancel,
            prompt_fallback_parent,
            wayland_swap_tx,
            wayland_installed: false,
            wayland_host_key: None,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            Input::ConfigChanged(config) => {
                if self.config == config {
                    return;
                }

                tracing::info!("app config changed");
                self.services
                    .broadcast(Control::Reconfigure(config.clone()));
                self.theme.reload(&config);
                self.theme.apply_configured_mode(&config.theme_mode);
                self.reconcile_panels(&config);
                self.config = config;
            }
            Input::ThemeReload => {
                tracing::info!("theme file changed, reloading");
                self.theme.reload(&self.config);
            }
            Input::ThemeChanged(state) => {
                if state.configured_mode != self.config.theme_mode {
                    tracing::debug!(
                        current_configured_mode = ?self.config.theme_mode,
                        stale_configured_mode = ?state.configured_mode,
                        stale_effective_mode = ?state.effective_mode,
                        "ignoring stale theme service state"
                    );
                    return;
                }
                tracing::debug!(
                    configured_mode = ?state.configured_mode,
                    effective_mode = ?state.effective_mode,
                    reason = ?state.reason,
                    "applying theme service state"
                );
                self.theme.apply_effective_mode(state.effective_mode);
                if let Err(error) = theme::sync_system_color_scheme(state.effective_mode) {
                    tracing::warn!(
                        ?error,
                        effective_mode = ?state.effective_mode,
                        "failed to sync system color scheme"
                    );
                }
            }
            Input::AppletDirsChanged(discovered) => {
                tracing::debug!("applet directories changed, reconciling panels");
                self.discovered_applets = discovered;
                let config = self.config.clone();
                self.reconcile_panels(&config);
            }
            Input::MonitorsChanged => {
                tracing::info!("monitors changed, reconciling panels");
                let config = self.config.clone();
                self.reconcile_panels(&config);
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.network_agent_cancel.cancel();
        self.bluetooth_agent_cancel.cancel();
    }
}

fn spawn_idle_subsystem(
    session: zbus::Connection,
) -> tokio::sync::mpsc::Sender<Box<dyn WaylandIdleInhibitor + Send>> {
    let own_unique_bus_name = session
        .unique_name()
        .map(|n| n.to_string())
        .unwrap_or_default();
    let (swap_tx, swap_rx) = tokio::sync::mpsc::channel::<Box<dyn WaylandIdleInhibitor + Send>>(1);
    let backend: Box<dyn WaylandIdleInhibitor + Send> = Box::new(NoopWaylandInhibitor);
    let initial_health = backend.health();
    let (health_tx, health_rx) = tokio::sync::watch::channel(initial_health);
    relm4::spawn(async move {
        let cancel = CancellationToken::new();
        match crate::dbus::idle_inhibitors::spawn(session, cancel.clone()).await {
            Ok(handle) => {
                let state_rx = handle.subscribe();
                let task_cancel = cancel.clone();
                tokio::spawn(async move {
                    wayland_idle_inhibit::run(backend, state_rx, swap_rx, health_tx, task_cancel)
                        .await;
                });
                if SHELL_EXTENSIONS
                    .set(ShellExtensions {
                        idle_inhibitor: handle,
                        wayland_health: health_rx,
                        own_unique_bus_name,
                    })
                    .is_err()
                {
                    tracing::warn!("idle SHELL_EXTENSIONS already initialized");
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = ?e,
                    "idle inhibitor proxy unavailable; idle applet disabled"
                );
            }
        }
    });
    swap_tx
}

fn spawn_theme_subscription(
    theme: glimpse_core::services::theme::ThemeHandle,
    sender: relm4::Sender<Input>,
) {
    relm4::spawn(async move {
        let mut state_rx = theme.subscribe();
        if sender
            .send(Input::ThemeChanged(*state_rx.borrow()))
            .is_err()
        {
            return;
        }

        loop {
            if state_rx.changed().await.is_err() {
                break;
            }
            if sender
                .send(Input::ThemeChanged(*state_rx.borrow()))
                .is_err()
            {
                break;
            }
        }
    });
}

impl App {
    fn reconcile_panels(&mut self, new_config: &Config) {
        let effective_applets =
            merge_applet_configs(&self.discovered_applets.normal, &new_config.applets);
        let services = self.services.handles();
        let monitors = list_gdk_monitors();

        let mut existing: HashMap<panels::PanelKey, PanelState> = self
            .panels
            .drain(..)
            .map(|state| (state.key.clone(), state))
            .collect();

        let mut new_panels: Vec<PanelState> = Vec::new();
        for (index, cfg) in new_config.panels.iter().enumerate() {
            for monitor in &monitors {
                let connector = monitor_connector(monitor);
                if let Some(target) = cfg.monitor.as_deref() {
                    if connector.as_deref() != Some(target) {
                        continue;
                    }
                }
                let key = panels::PanelKey {
                    index,
                    monitor: connector.clone().unwrap_or_default(),
                    position: cfg.position.clone(),
                };
                let state = match existing.remove(&key) {
                    Some(state) => {
                        state.controller.emit(panels::Input::Reconfigure(
                            panels::PanelRuntimeConfig {
                                config: cfg.clone(),
                                applet_configs: effective_applets.clone(),
                            },
                        ));
                        state
                    }
                    None => build_panel(
                        index,
                        cfg.clone(),
                        services.clone(),
                        monitor.clone(),
                        effective_applets.clone(),
                    ),
                };
                new_panels.push(state);
            }
        }
        self.panels = new_panels;
        self.update_prompt_parent(new_config);

        for (key, state) in existing.drain() {
            state.controller.widget().destroy();
            tracing::debug!(
                ?key.position,
                index = key.index,
                monitor = %key.monitor,
                "panel removed"
            );
        }

        if let Some(host) = &self.wayland_host_key
            && !self.panels.iter().any(|p| &p.key == host)
        {
            tracing::debug!(
                monitor = %host.monitor,
                "wayland idle inhibitor host panel gone, rebinding"
            );
            self.wayland_installed = false;
            self.wayland_host_key = None;
        }

        self.maybe_install_wayland_inhibitor();
    }

    fn maybe_install_wayland_inhibitor(&mut self) {
        if self.wayland_installed {
            return;
        }
        let Some(panel) = self.panels.first() else {
            return;
        };
        let window: gtk4::Window = panel.controller.widget().clone().upcast();
        let host_key = panel.key.clone();
        let swap_tx = self.wayland_swap_tx.clone();
        let install = move |window: &gtk4::Window| -> Result<(), String> {
            match GdkWaylandInhibitor::try_new(window) {
                Ok(backend) => {
                    let boxed: Box<dyn WaylandIdleInhibitor + Send> = Box::new(backend);
                    if let Err(e) = swap_tx.try_send(boxed) {
                        tracing::warn!(?e, "failed to install wayland idle inhibitor backend");
                        Err(format!("{e:?}"))
                    } else {
                        tracing::info!("installed real wayland idle inhibitor backend");
                        Ok(())
                    }
                }
                Err(message) => {
                    tracing::warn!(
                        %message,
                        "wayland idle inhibit unavailable; staying on Noop backend"
                    );
                    Err(message)
                }
            }
        };
        if window.is_mapped() {
            if install(&window).is_ok() {
                self.wayland_installed = true;
                self.wayland_host_key = Some(host_key);
            }
        } else {
            let install_once = std::cell::Cell::new(Some(install));
            self.wayland_installed = true;
            self.wayland_host_key = Some(host_key);
            window.connect_map(move |w| {
                if let Some(install) = install_once.take() {
                    let _ = install(w);
                }
            });
        }
    }

    fn update_prompt_parent(&self, config: &Config) {
        let parent = self
            .panels
            .first()
            .map(|panel| panel.controller.widget().clone().upcast())
            .unwrap_or_else(|| self.prompt_fallback_parent.clone());

        let _ = config;
        let theme_mode = theme::DIALOG_THEME_MODE;
        theme::apply_theme_mode(&self.prompt_fallback_parent, &theme_mode);
        self.network_prompt_host
            .emit(network_prompts::PromptHostInput::SetParent(parent.clone()));
        self.network_prompt_host
            .emit(network_prompts::PromptHostInput::SetThemeMode(theme_mode));
        self.bluetooth_prompt_host
            .emit(bluetooth_prompts::PromptHostInput::SetParent(parent));
        self.bluetooth_prompt_host
            .emit(bluetooth_prompts::PromptHostInput::SetThemeMode(theme_mode));
    }
}

struct PanelState {
    pub key: panels::PanelKey,
    pub controller: Controller<panels::Panel>,
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

fn build_panel(
    index: usize,
    config: PanelConfig,
    services: Services,
    monitor: gdk::Monitor,
    applet_configs: glimpse_core::AppletConfigs,
) -> PanelState {
    let key = panels::PanelKey {
        index,
        monitor: monitor_connector(&monitor).unwrap_or_default(),
        position: config.position.clone(),
    };
    let monitor_connector = monitor_connector(&monitor);
    let controller = panels::Panel::builder()
        .launch(panels::Init {
            config,
            services: services.clone(),
            monitor: Some(monitor),
            monitor_connector,
            applet_configs,
        })
        .detach();
    PanelState { key, controller }
}
