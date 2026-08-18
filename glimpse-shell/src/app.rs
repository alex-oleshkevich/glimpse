use std::collections::HashMap;

use crate::{
    agents::{bluetooth::BluetoothAgentRuntime, network::NetworkAgentRuntime},
    ipc::{IpcEmitter, IpcHandle, launch as launch_ipc},
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
    Config, ConfigEvent, DiscoveredApplets, PanelConfig, Position,
    config::merge_applet_configs,
    expand_dev_slot,
    services::idle_inhibitor::{self, BackendHealth, HealthKind, SourceKind},
    services::theme::State as ThemeServiceState,
    watch_for_config_changes,
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
    WaylandInstalled(panels::PanelKey),
    WaylandInstallFailed,
    /// Broadcast to every panel; whichever one owns the applet replies.
    TogglePopover {
        applet: String,
        section: Option<panels::PanelSection>,
        occurrence: Option<usize>,
        reply: tokio::sync::mpsc::UnboundedSender<Result<(), String>>,
    },
}

pub struct App {
    config: Config,
    discovered_applets: DiscoveredApplets,
    services: ServiceRuntime,
    ipc: IpcHandle,
    theme: ThemeState,
    panels: Vec<PanelState>,
    network_prompt_host: Controller<network_prompts::PromptHost>,
    bluetooth_prompt_host: Controller<bluetooth_prompts::PromptHost>,
    network_agent_cancel: CancellationToken,
    bluetooth_agent_cancel: CancellationToken,
    prompt_fallback_parent: gtk4::Widget,
    wayland_swap_tx: tokio::sync::mpsc::Sender<Box<dyn WaylandIdleInhibitor + Send>>,
    services_started: bool,
    wayland_installed: bool,
    wayland_pending: bool,
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

        let idle_system_dbus = system_dbus.clone();
        let (bluetooth_agent_runtime, bluetooth_agent) = BluetoothAgentRuntime::new(system_dbus);
        let bluetooth_agent_cancel = CancellationToken::new();
        {
            let cancel = bluetooth_agent_cancel.clone();
            relm4::spawn(async move {
                bluetooth_agent_runtime.run(cancel).await;
            });
        }

        let idle_session_dbus = init.dbus.session.clone();
        let services = ServiceRuntime::new(init.dbus);
        let ipc = launch_ipc(&services.handles(), sender.input_sender().clone());
        let wayland_swap_tx =
            spawn_idle_subsystem(idle_session_dbus, idle_system_dbus, ipc.emitter());
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
            ipc,
            theme,
            panels: vec![],
            network_prompt_host,
            bluetooth_prompt_host,
            network_agent_cancel,
            bluetooth_agent_cancel,
            prompt_fallback_parent,
            wayland_swap_tx,
            services_started: false,
            wayland_installed: false,
            wayland_pending: false,
            wayland_host_key: None,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Input::ConfigChanged(config) => {
                if self.config == config {
                    return;
                }

                tracing::info!("app config changed");
                if self.services_started {
                    self.services
                        .broadcast(Control::Reconfigure(config.clone()));
                }
                self.theme.reload(&config);
                self.theme.apply_configured_mode(&config.theme_mode);
                self.reconcile_panels(&config, &sender);
                self.start_services_if_needed(&config);
                self.config = config;
                self.ipc.emit("config.changed", vec![]);
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
                diff_discovered(&self.ipc, &self.discovered_applets, &discovered);
                self.discovered_applets = discovered;
                let config = self.config.clone();
                self.reconcile_panels(&config, &sender);
            }
            Input::MonitorsChanged => {
                tracing::info!("monitors changed, reconciling panels");
                let config = self.config.clone();
                self.reconcile_panels(&config, &sender);
                self.start_services_if_needed(&config);
            }
            Input::WaylandInstalled(host_key) => {
                self.wayland_pending = false;
                self.wayland_installed = true;
                self.wayland_host_key = Some(host_key);
            }
            Input::WaylandInstallFailed => {
                // The deferred install (on window map) failed. Clear the pending
                // flag so the next reconcile retries instead of leaving the
                // shell on the Noop inhibitor backend forever.
                self.wayland_pending = false;
                self.wayland_host_key = None;
            }
            Input::TogglePopover {
                applet,
                section,
                occurrence,
                reply,
            } => {
                for panel in &self.panels {
                    panel.controller.emit(panels::Input::TogglePopover {
                        applet: applet.clone(),
                        section: section.clone(),
                        occurrence,
                        reply: reply.clone(),
                    });
                }
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
    system_dbus: zbus::Connection,
    ipc: IpcEmitter,
) -> tokio::sync::mpsc::Sender<Box<dyn WaylandIdleInhibitor + Send>> {
    let own_unique_bus_name = session
        .unique_name()
        .map(|n| n.to_string())
        .unwrap_or_default();
    // Small buffer so a backend swap is not dropped if the inhibitor task is
    // briefly busy; a dropped swap would strand the panel on the Noop backend.
    let (swap_tx, swap_rx) = tokio::sync::mpsc::channel::<Box<dyn WaylandIdleInhibitor + Send>>(4);
    let backend: Box<dyn WaylandIdleInhibitor + Send> = Box::new(NoopWaylandInhibitor);
    let initial_health = backend.health();
    let (health_tx, health_rx) = tokio::sync::watch::channel(initial_health);
    relm4::spawn(async move {
        let cancel = CancellationToken::new();
        match crate::dbus::idle_inhibitors::spawn(session, system_dbus, cancel.clone()).await {
            Ok(handle) => {
                let state_rx = handle.subscribe();
                spawn_idle_inhibitor_ipc_watcher(handle.subscribe(), ipc);
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

fn spawn_idle_inhibitor_ipc_watcher(
    mut rx: tokio::sync::watch::Receiver<idle_inhibitor::State>,
    ipc: IpcEmitter,
) {
    relm4::spawn(async move {
        let mut prev = rx.borrow_and_update().clone();
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let next = rx.borrow_and_update().clone();

            let prev_ids: std::collections::HashSet<u64> =
                prev.inhibitors.iter().map(|record| record.id).collect();
            let next_ids: std::collections::HashSet<u64> =
                next.inhibitors.iter().map(|record| record.id).collect();

            for record in &next.inhibitors {
                if !prev_ids.contains(&record.id) {
                    ipc.emit(
                        "idle.inhibitor_added",
                        vec![
                            ("id", record.id.to_string()),
                            ("who", record.who.clone()),
                            ("why", record.why.clone()),
                            ("source", source_name(&record.source.kind).to_owned()),
                        ],
                    );
                }
            }
            for record in &prev.inhibitors {
                if !next_ids.contains(&record.id) {
                    ipc.emit(
                        "idle.inhibitor_removed",
                        vec![("id", record.id.to_string()), ("who", record.who.clone())],
                    );
                }
            }

            emit_health_change(
                &ipc,
                "screen_saver",
                &prev.health.screen_saver,
                &next.health.screen_saver,
            );
            emit_health_change(&ipc, "portal", &prev.health.portal, &next.health.portal);
            emit_health_change(&ipc, "login1", &prev.health.login1, &next.health.login1);

            prev = next;
        }
    });
}

fn emit_health_change(ipc: &IpcEmitter, backend: &str, prev: &BackendHealth, next: &BackendHealth) {
    if prev != next {
        ipc.emit(
            "idle.backend_health_changed",
            vec![
                ("backend", backend.to_owned()),
                ("health", backend_health_name(next).to_owned()),
            ],
        );
    }
}

fn source_name(kind: &SourceKind) -> &'static str {
    match kind {
        SourceKind::ScreenSaver => "screen_saver",
        SourceKind::Portal => "portal",
        SourceKind::Login1 => "login1",
    }
}

fn backend_health_name(health: &BackendHealth) -> &'static str {
    match health.kind {
        HealthKind::Ready => "ready",
        HealthKind::Degraded => "degraded",
        HealthKind::Unsupported => "unsupported",
    }
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
    fn reconcile_panels(&mut self, new_config: &Config, sender: &ComponentSender<Self>) {
        // Merge normal + dev discovered applets; explicit config entries win.
        let mut all_discovered = self.discovered_applets.normal.clone();
        all_discovered.extend(self.discovered_applets.dev.clone());
        let effective_applets = merge_applet_configs(&all_discovered, &new_config.applets);

        // Sorted dev names used to expand __dev__ in each panel's slot lists.
        let mut dev_names: Vec<String> = self.discovered_applets.dev.keys().cloned().collect();
        dev_names.sort();

        let services = self.services.handles();
        let monitors = list_gdk_monitors();

        let mut existing: HashMap<panels::PanelKey, PanelState> = self
            .panels
            .drain(..)
            .map(|state| (state.key.clone(), state))
            .collect();

        let mut new_panels: Vec<PanelState> = Vec::new();
        for (index, cfg) in new_config.panels.iter().enumerate() {
            let expanded = expand_dev_slot(cfg, &dev_names);
            let theme_mode = cfg.effective_theme_mode(new_config.theme_mode);
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
                                config: expanded.clone(),
                                theme_mode,
                                applet_configs: effective_applets.clone(),
                            },
                        ));
                        state
                    }
                    None => {
                        emit_panel(&self.ipc, "panel.added", &key);
                        build_panel(
                            key,
                            expanded.clone(),
                            theme_mode,
                            services.clone(),
                            monitor.clone(),
                            effective_applets.clone(),
                            self.ipc.emitter(),
                        )
                    }
                };
                new_panels.push(state);
            }
        }
        self.panels = new_panels;
        self.update_prompt_parent();

        for (key, state) in existing.drain() {
            state.controller.widget().destroy();
            emit_panel(&self.ipc, "panel.removed", &key);
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
            self.wayland_pending = false;
            self.wayland_host_key = None;
        }

        self.maybe_install_wayland_inhibitor(sender);
    }

    fn start_services_if_needed(&mut self, config: &Config) {
        if self.services_started {
            return;
        }
        self.services.broadcast(Control::Start(config.clone()));
        self.services_started = true;
    }

    fn maybe_install_wayland_inhibitor(&mut self, sender: &ComponentSender<Self>) {
        if self.wayland_installed || self.wayland_pending {
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
            self.wayland_pending = true;
            self.wayland_host_key = Some(host_key.clone());
            let input_sender = sender.input_sender().clone();
            let install_once = std::cell::Cell::new(Some(install));
            window.connect_map(move |w| {
                if let Some(install) = install_once.take() {
                    if install(w).is_ok() {
                        let _ = input_sender.send(Input::WaylandInstalled(host_key.clone()));
                    } else {
                        let _ = input_sender.send(Input::WaylandInstallFailed);
                    }
                }
            });
        }
    }

    fn update_prompt_parent(&self) {
        let parent = self
            .panels
            .first()
            .map(|panel| panel.controller.widget().clone().upcast())
            .unwrap_or_else(|| self.prompt_fallback_parent.clone());

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
    key: panels::PanelKey,
    config: PanelConfig,
    theme_mode: glimpse_core::ThemeMode,
    services: Services,
    monitor: gdk::Monitor,
    applet_configs: glimpse_core::AppletConfigs,
    ipc: IpcEmitter,
) -> PanelState {
    let monitor_connector = monitor_connector(&monitor);
    let controller = panels::Panel::builder()
        .launch(panels::Init {
            config,
            theme_mode,
            services: services.clone(),
            monitor: Some(monitor),
            monitor_connector,
            applet_configs,
            ipc,
        })
        .detach();
    PanelState { key, controller }
}

fn position_name(position: &Position) -> &'static str {
    match position {
        Position::Left => "left",
        Position::Top => "top",
        Position::Right => "right",
        Position::Bottom => "bottom",
    }
}

fn emit_panel(ipc: &IpcHandle, event: &str, key: &panels::PanelKey) {
    ipc.emit(
        event,
        vec![
            ("index", key.index.to_string()),
            ("monitor", key.monitor.clone()),
            ("position", position_name(&key.position).to_owned()),
        ],
    );
}

/// Emit `applet.discovered name=<> kind=<normal|dev> change=<added|updated|removed>`
/// for every applet package that appeared, changed, or vanished on disk.
fn diff_discovered(ipc: &IpcHandle, old: &DiscoveredApplets, new: &DiscoveredApplets) {
    for (kind, prev, next) in [
        ("normal", &old.normal, &new.normal),
        ("dev", &old.dev, &new.dev),
    ] {
        for (name, cfg) in next {
            let change = match prev.get(name) {
                None => "added",
                Some(p) if p != cfg => "updated",
                Some(_) => continue,
            };
            ipc.emit(
                "applet.discovered",
                vec![
                    ("name", name.clone()),
                    ("kind", kind.to_owned()),
                    ("change", change.to_owned()),
                ],
            );
        }
        for name in prev.keys() {
            if !next.contains_key(name) {
                ipc.emit(
                    "applet.discovered",
                    vec![
                        ("name", name.clone()),
                        ("kind", kind.to_owned()),
                        ("change", "removed".to_owned()),
                    ],
                );
            }
        }
    }
}
