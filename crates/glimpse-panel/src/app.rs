use adw::gdk::{self, prelude::*};
use adw::prelude::{AdwDialogExt, AlertDialogExt};
use futures_util::StreamExt;
use gettextrs::gettext;
use glib::prelude::ToVariant;
use gtk4::gio;
use gtk4::prelude::{ActionMapExt, GtkWindowExt, WidgetExt};
use std::collections::HashMap;
use zeroize::Zeroizing;

use glimpse_config::{
    Config, Documents, PANEL_STYLESHEET, Reload, Theme, unknown_applets, watch_config, watch_theme,
};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    Answer, BluetoothError, BluetoothHandle, CommandError, ExecState, NetworkError, NetworkHandle,
    SessionAction, SessionActionsHandle,
};
use glimpse_widgets::actions::ComponentActions;
use glimpse_widgets::color_scheme;
use glimpse_widgets::{
    NetworkEntered, PairingAnswer, PairingDialog, PairingEntry as Entry, SecretAnswer,
    SecretDialog, Styles,
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::{
    applet::{Report, spawn_reported},
    applets::bluetooth::{NAME_CAP, prompt_entry},
    attached::{AttachedRegistry, SharedRegistry},
    components::{self, panel},
    config_alert::{self, Problems},
    services::PanelServices,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretPrompt {
    key: String,
    name: String,
    retry: bool,
    entered: NetworkEntered,
}

#[derive(Clone, Debug)]
pub struct SessionDialog {
    pub title: String,
    pub body: String,
    pub accept: String,
    pub action: SessionAction,
}

pub struct AppInit {
    pub config: Config,
    pub theme: Theme,
    pub failure: Option<String>,
    pub panels_only: bool,
}

type Pairing = (Entry, String, String, String);

#[allow(clippy::large_enum_variant, clippy::enum_variant_names)]
pub enum AppInput {
    BluetoothPrompt(Option<Pairing>),
    NetworkSecret(Option<SecretPrompt>),
    SessionConfirm(SessionDialog),
    SessionRun(SessionAction),
    ConfigChanged(Config),
    ConfigRejected(String),
    ConfigRecovered,
    ClosePopovers,
    MonitorsChanged,
    ServicesReady(PanelServices),
    Theme(Reload<Theme>),
    Attached,
    Detached,
}

impl std::fmt::Debug for AppInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            AppInput::BluetoothPrompt(_) => "BluetoothPrompt",
            AppInput::NetworkSecret(_) => "NetworkSecret",
            AppInput::SessionConfirm(_) => "SessionConfirm",
            AppInput::SessionRun(_) => "SessionRun",
            AppInput::ConfigChanged(_) => "ConfigChanged",
            AppInput::ConfigRejected(_) => "ConfigRejected",
            AppInput::ConfigRecovered => "ConfigRecovered",
            AppInput::ClosePopovers => "ClosePopovers",
            AppInput::MonitorsChanged => "MonitorsChanged",
            AppInput::ServicesReady(_) => "ServicesReady",
            AppInput::Theme(_) => "Theme",
            AppInput::Attached => "Attached",
            AppInput::Detached => "Detached",
        })
    }
}

pub struct App {
    config: Config,
    panels: Vec<PanelState>,
    theme_watch: JoinHandle<()>,
    styles: Styles,
    theme: Theme,
    services: Option<PanelServices>,
    services_start: JoinHandle<()>,
    bluetooth_watch: Option<JoinHandle<()>>,
    network_watch: Option<JoinHandle<()>>,
    exec_watch: Option<JoinHandle<()>>,
    attached: SharedRegistry,
    pairing: Option<(PairingDialog, glib::SignalHandlerId)>,
    secret: Option<(SecretDialog, glib::SignalHandlerId)>,
    problems: Problems,
    actions: ComponentActions,
    bars: gio::SimpleAction,
    alert: Option<watch::Sender<Option<String>>>,
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
            set_default_size: (420, 260),
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        tracing::info!("initializing app");
        watch_monitors(sender.clone());
        let theme_watch = spawn_theme_watch(
            &init.config.appearance.theme,
            init.theme.clone(),
            sender.clone(),
        );
        let services_start = spawn_services(init.config.clone(), sender.clone());
        spawn_config_watch(init.config.clone(), init.failure.clone(), sender.clone());

        let problems = Problems {
            failure: init.failure,
            panels_only: init.panels_only,
            rejected: None,
            theme: None,
            unknown_applets: unknown_applets(&init.config),
        };
        let actions = ComponentActions::install(
            &relm4::main_application(),
            crate::cli::build::PKG_VERSION,
            problems.verdict(),
        );
        let action = gio::SimpleAction::new("close-popovers", None);
        let input = sender.input_sender().clone();
        action.connect_activate(move |_, _| {
            let _ = input.send(AppInput::ClosePopovers);
        });
        relm4::main_application().add_action(&action);
        let bars = gio::SimpleAction::new_stateful(
            "bars",
            None,
            &Vec::<(u32, String, String)>::new().to_variant(),
        );
        bars.set_enabled(false);
        bars.connect_change_state(|_, _| {});
        relm4::main_application().add_action(&bars);

        let styles = Styles::install(color_scheme(init.config.appearance.color_scheme));
        let attached = SharedRegistry::default();
        let model = App {
            config: init.config,
            panels: Default::default(),
            theme_watch,
            theme: init.theme,
            styles,
            services: None,
            services_start,
            bluetooth_watch: None,
            network_watch: None,
            exec_watch: None,
            attached,
            pairing: None,
            secret: None,
            problems,
            actions,
            bars,
            alert: None,
        };
        model.reload_styles();

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        let mut export = false;
        match msg {
            AppInput::ConfigChanged(config) => {
                let renamed = config.appearance.theme != self.config.appearance.theme;
                glimpse_utils::report_language_change(
                    self.config.regional.language(),
                    config.regional.language(),
                );
                self.config = config;
                self.problems.recover();
                if renamed {
                    self.problems.theme = None;
                }
                self.problems.unknown_applets = unknown_applets(&self.config);
                self.publish_problems();
                if let Some(services) = &self.services {
                    services.reconfigure(&self.config);
                }
                self.styles
                    .set_color_scheme(color_scheme(self.config.appearance.color_scheme));
                if renamed {
                    self.theme_watch.abort();
                    self.theme_watch = spawn_theme_watch(
                        &self.config.appearance.theme,
                        self.theme.clone(),
                        sender.clone(),
                    );
                }
            }
            AppInput::ConfigRejected(reason) => {
                self.problems.reject(reason);
                self.publish_problems();
            }
            AppInput::ConfigRecovered => {
                self.problems.recover();
                self.publish_problems();
            }
            AppInput::ClosePopovers => {
                self.close_popovers();
                return;
            }
            AppInput::MonitorsChanged => {}
            AppInput::ServicesReady(services) => {
                services.reconfigure(&self.config);
                self.bluetooth_watch = Some(spawn_bluetooth_watch(
                    services.handles.bluetooth.clone(),
                    sender.clone(),
                ));
                self.network_watch = Some(spawn_network_watch(
                    services.handles.network.clone(),
                    sender.clone(),
                ));
                self.exec_watch = Some(spawn_exec_watch(
                    services.handles.exec.clone(),
                    self.attached.clone(),
                    sender.clone(),
                ));
                self.alert = Some(config_alert::spawn(
                    services.handles.notifications.clone(),
                    self.problems.body(),
                ));
                self.services = Some(services);
                export = true;
            }
            AppInput::Attached | AppInput::Detached => {}
            AppInput::BluetoothPrompt(pairing) => {
                if let Some(services) = &self.services {
                    let bluetooth = services.handles.bluetooth.clone();
                    let notifications = services.handles.notifications.clone();
                    self.show_pairing(pairing, &bluetooth, notifications);
                }
                return;
            }
            AppInput::NetworkSecret(request) => {
                if let Some(services) = &self.services {
                    let network = services.handles.network.clone();
                    let notifications = services.handles.notifications.clone();
                    self.show_secret(request, &network, notifications);
                }
                return;
            }
            AppInput::SessionConfirm(request) => {
                self.close_popovers();
                self.show_session_dialog(request);
                return;
            }
            AppInput::SessionRun(action) => {
                self.close_popovers();
                self.run_session_action(action);
                return;
            }
            AppInput::Theme(reload) => {
                match reload {
                    Reload::Changed(theme) => {
                        self.theme = theme;
                        self.problems.theme = None;
                        self.reload_styles();
                    }
                    Reload::Recovered => self.problems.theme = None,
                    Reload::Rejected(reason) => self.problems.theme = Some(reason),
                }
                self.publish_problems();
            }
        }
        reconcile_panels(
            &mut self.panels,
            &self.config,
            self.services.as_ref(),
            &self.attached,
            sender.input_sender().clone(),
        );
        self.bars.set_state(&bars_state(
            &self.config,
            self.panels.iter().map(|panel| &panel.key),
        ));
        if export
            && let Some(connection) = self
                .services
                .as_ref()
                .and_then(|services| services.session.clone())
        {
            let registry = self.attached.clone();
            let exec = self
                .services
                .as_ref()
                .map(|services| services.handles.exec.clone());
            let input = sender.input_sender().clone();
            if let Some(exec) = exec {
                relm4::spawn(async move {
                    crate::attached::export(&connection, registry, exec, input).await
                });
            }
        }
        self.styles
            .set_variant(&self.config.appearance.theme_variant);
        self.styles
            .set_animation_speed(self.config.appearance.animation_speed);
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        self.services_start.abort();
        if let Some(watch) = self.bluetooth_watch.take() {
            watch.abort();
        }
        if let Some(watch) = self.network_watch.take() {
            watch.abort();
        }
        if let Some(watch) = self.exec_watch.take() {
            watch.abort();
        }
        if let Some(services) = self.services.take() {
            relm4::spawn(services.shutdown());
        }
    }
}

impl App {
    fn publish_problems(&self) {
        if let Some(alert) = &self.alert {
            alert.send_replace(self.problems.body());
        }
        self.actions.report(self.problems.verdict());
    }

    fn close_popovers(&self) {
        for state in &self.panels {
            state.controller.emit(panel::Input::ClosePopover);
        }
    }

    fn show_pairing(
        &mut self,
        pairing: Option<Pairing>,
        bluetooth: &BluetoothHandle,
        notifications: NotificationsProviderHandle,
    ) {
        let Some((entry, code, device, name)) = pairing else {
            close(self.pairing.take());
            return;
        };

        let dialog = match &self.pairing {
            Some((dialog, _)) if is_open(dialog) => dialog.clone(),
            _ => {
                self.close_popovers();
                let dialog = PairingDialog::new();
                let handle = bluetooth.clone();
                let answered = dialog.connect_answered(move |_, answer| {
                    let handle = handle.clone();
                    let answer = answered(answer);
                    spawn_reported(
                        "bluetooth.answer_pairing",
                        prompt_report(notifications.clone(), gettext("Bluetooth")),
                        |_: &BluetoothError| Some(gettext("The pairing answer was not delivered.")),
                        async move { handle.answer_pairing(answer).await },
                    );
                });
                dialog.set_title(&gettext("Bluetooth pairing"));
                dialog.present(None::<&gtk4::Widget>);
                self.pairing = Some((dialog.clone(), answered));
                dialog
            }
        };

        dialog.ask(&device, &name, entry, &code);
    }

    /// The prompt is presented here rather than by the applet: an applet holds no state outliving
    /// its widget, and NetworkManager can ask for a secret at any moment — including while the
    /// popover is shut.
    fn show_secret(
        &mut self,
        request: Option<SecretPrompt>,
        network: &NetworkHandle,
        notifications: NotificationsProviderHandle,
    ) {
        let Some(SecretPrompt {
            key,
            name,
            retry,
            entered,
        }) = request
        else {
            close(self.secret.take());
            return;
        };

        let dialog = match &self.secret {
            Some((dialog, _)) if is_open(dialog) => dialog.clone(),
            _ => {
                self.close_popovers();
                let dialog = SecretDialog::new();
                let handle = network.clone();
                let answered = dialog.connect_answered(move |_, answer| {
                    let handle = handle.clone();
                    let answer = match answer {
                        SecretAnswer::Secret(secret) => {
                            glimpse_services::SecretAnswer::Secret(Zeroizing::new(secret))
                        }
                        SecretAnswer::Refused => glimpse_services::SecretAnswer::Refused,
                    };
                    spawn_reported(
                        "network.answer_secret",
                        prompt_report(notifications.clone(), gettext("Network")),
                        |_: &NetworkError| Some(gettext("The password was not delivered.")),
                        async move { handle.answer_secret(answer).await },
                    );
                });
                dialog.set_title(&gettext("Network password"));
                dialog.present(None::<&gtk4::Widget>);
                self.secret = Some((dialog.clone(), answered));
                dialog
            }
        };

        dialog.ask(&key, &name, retry, entered);
    }

    fn show_session_dialog(&self, request: SessionDialog) {
        let dialog = adw::AlertDialog::new(Some(&request.title), Some(&request.body));
        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("accept", &request.accept);
        dialog.set_response_appearance("accept", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("accept"));
        dialog.set_close_response("cancel");
        let action = request.action.clone();
        let actions = self
            .services
            .as_ref()
            .map(|services| services.handles.session_actions.clone());
        let notifications = self
            .services
            .as_ref()
            .map(|services| services.handles.notifications.clone());
        dialog.connect_response(None, move |_, response| {
            if session_accepted(response)
                && let (Some(actions), Some(notifications)) =
                    (actions.clone(), notifications.clone())
            {
                execute_session(actions, notifications, action.clone());
            }
        });
        dialog.set_title(&request.title);
        dialog.present(None::<&gtk4::Widget>);
    }

    fn run_session_action(&self, action: SessionAction) {
        let Some(services) = &self.services else {
            return;
        };
        execute_session(
            services.handles.session_actions.clone(),
            services.handles.notifications.clone(),
            action,
        );
    }

    fn reload_styles(&self) {
        let appearance = &self.config.appearance;
        self.styles.load(&self.theme);
        self.styles.set_variant(&appearance.theme_variant);
        self.styles.set_animation_speed(appearance.animation_speed);
    }
}

fn session_accepted(response: &str) -> bool {
    response == "accept"
}

fn execute_session(
    actions: SessionActionsHandle,
    notifications: NotificationsProviderHandle,
    action: SessionAction,
) {
    spawn_reported(
        "session.run_action",
        Report {
            notifications,
            app_name: gettext("Session"),
            icon: "system-shutdown-symbolic".to_owned(),
            summary: gettext("Could not complete the session action"),
        },
        |_: &CommandError| Some(gettext("The session manager refused that action.")),
        async move { actions.run(action).await },
    );
}

fn prompt_report(notifications: NotificationsProviderHandle, app_name: String) -> Report {
    Report {
        notifications,
        app_name,
        icon: "dialog-password-symbolic".to_owned(),
        summary: gettext("Could not answer the request"),
    }
}

fn is_open(dialog: &impl IsA<adw::Dialog>) -> bool {
    dialog.as_ref().parent().is_some()
}

fn close<D: IsA<adw::Dialog>>(open: Option<(D, glib::SignalHandlerId)>) {
    let Some((dialog, handler)) = open else {
        return;
    };
    dialog.block_signal(&handler);
    dialog.as_ref().force_close();
    dialog.unblock_signal(&handler);
}

fn spawn_services(config: Config, sender: ComponentSender<App>) -> JoinHandle<()> {
    relm4::spawn(async move {
        let services = PanelServices::start(&config).await;
        sender.input(AppInput::ServicesReady(services));
    })
}

fn answered(answer: PairingAnswer) -> Answer {
    match answer {
        PairingAnswer::Deny => Answer::Deny,
        PairingAnswer::Accept => Answer::Confirm,
        PairingAnswer::Pin(pin) => Answer::Pin(pin),
        PairingAnswer::Passkey(passkey) => Answer::Passkey(passkey),
    }
}

fn spawn_network_watch(network: NetworkHandle, sender: ComponentSender<App>) -> JoinHandle<()> {
    relm4::spawn(async move {
        let mut states = network.subscribe();
        let mut last = None;
        loop {
            let next = {
                let state = states.borrow_and_update();
                state.secret.as_ref().map(|request| SecretPrompt {
                    key: format!("{}:{}", request.setting, request.name),
                    name: request.name.clone(),
                    retry: request.retry,
                    entered: crate::applets::network::entered_for(&request.setting),
                })
            };
            if next != last {
                last = next.clone();
                sender.input(AppInput::NetworkSecret(next));
            }
            if states.changed().await.is_err() {
                break;
            }
        }
    })
}

fn spawn_bluetooth_watch(
    bluetooth: BluetoothHandle,
    sender: ComponentSender<App>,
) -> JoinHandle<()> {
    relm4::spawn(async move {
        let mut states = bluetooth.subscribe();
        let mut last = None;
        loop {
            let next = {
                let state = states.borrow_and_update();
                state.pairing.as_ref().map(|prompt| {
                    let name = state
                        .name(prompt.device())
                        .filter(|name| !name.is_empty())
                        .map_or_else(
                            || gettext("this device"),
                            |name| glimpse_utils::clean(name, NAME_CAP),
                        );
                    let (entry, code) = prompt_entry(prompt);
                    (entry, code, prompt.device().as_str().to_owned(), name)
                })
            };
            if next != last {
                last = next.clone();
                sender.input(AppInput::BluetoothPrompt(next));
            }
            if states.changed().await.is_err() {
                break;
            }
        }
    })
}

fn spawn_exec_watch(
    exec: glimpse_services::ExecHandle,
    registry: SharedRegistry,
    sender: ComponentSender<App>,
) -> JoinHandle<()> {
    relm4::spawn(async move {
        let mut states = exec.subscribe();
        let mut seen = std::collections::BTreeSet::new();
        loop {
            let ended = {
                let state = states.borrow_and_update();
                let Ok(mut registry) = registry.lock() else {
                    break;
                };
                let ended = ended_dev(&state, &registry, &mut seen);
                for handle in &ended {
                    registry.remove(handle);
                }
                !ended.is_empty()
            };
            if ended {
                sender.input(AppInput::Detached);
            }
            if states.changed().await.is_err() {
                break;
            }
        }
    })
}

fn ended_dev(
    state: &ExecState,
    registry: &AttachedRegistry,
    seen: &mut std::collections::BTreeSet<String>,
) -> Vec<String> {
    seen.extend(
        registry
            .applets
            .iter()
            .filter(|applet| state.dev.contains(&applet.dev_key))
            .map(|applet| applet.handle.clone()),
    );
    seen.retain(|handle| {
        registry
            .applets
            .iter()
            .any(|applet| &applet.handle == handle)
    });
    registry
        .applets
        .iter()
        .filter(|applet| seen.contains(&applet.handle) && !state.dev.contains(&applet.dev_key))
        .map(|applet| applet.handle.clone())
        .collect()
}

fn spawn_theme_watch(name: &str, current: Theme, sender: ComponentSender<App>) -> JoinHandle<()> {
    let themes = watch_theme(name, PANEL_STYLESHEET, current);
    relm4::spawn(async move {
        let mut themes = Box::pin(themes);
        while let Some(reload) = themes.next().await {
            sender.input(AppInput::Theme(reload));
        }
    })
}

fn spawn_config_watch(current: Config, failure: Option<String>, sender: ComponentSender<App>) {
    relm4::spawn(async move {
        let mut configs = Box::pin(watch_config(Documents::ConfigAndPanel, current, failure));
        while let Some(reloaded) = configs.next().await {
            sender.input(match reloaded {
                Reload::Changed(config) => AppInput::ConfigChanged(config),
                Reload::Recovered => AppInput::ConfigRecovered,
                Reload::Rejected(reason) => AppInput::ConfigRejected(reason),
            });
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    pub index: usize,
    pub monitor: String,
}

struct PanelState {
    pub key: Key,
    pub controller: Controller<components::panel::Panel>,
}

fn bars_state<'a>(config: &Config, keys: impl Iterator<Item = &'a Key>) -> glib::Variant {
    keys.filter_map(|key| {
        let panel = config.panels.get(key.index)?;
        let position = match panel.position {
            glimpse_config::Position::Top => "top",
            glimpse_config::Position::Bottom => "bottom",
            glimpse_config::Position::Left => "left",
            glimpse_config::Position::Right => "right",
        };
        Some((
            u32::try_from(key.index).ok()?,
            key.monitor.clone(),
            position.to_owned(),
        ))
    })
    .collect::<Vec<_>>()
    .to_variant()
}

fn reconcile_panels(
    panels: &mut Vec<PanelState>,
    config: &Config,
    services: Option<&PanelServices>,
    attached: &SharedRegistry,
    dialog: relm4::Sender<AppInput>,
) {
    let Some(services) = services else {
        return;
    };
    tracing::debug!("reconciling panels");
    let mut existing: HashMap<Key, PanelState> = panels
        .drain(..)
        .map(|state| (state.key.clone(), state))
        .collect();

    let monitors = list_gdk_monitors();
    let layout: Vec<Vec<String>> = config
        .panels
        .iter()
        .map(|panel| {
            let mut connectors: Vec<String> = monitors
                .iter()
                .filter_map(|monitor| monitor.connector().map(String::from))
                .filter(|connector| {
                    panel
                        .monitor
                        .as_deref()
                        .is_none_or(|target| target == connector)
                })
                .collect();
            connectors.sort();
            connectors
        })
        .collect();
    let configured = config
        .panels
        .iter()
        .map(|panel| {
            [
                (glimpse_services::Zone::Left, &panel.left),
                (glimpse_services::Zone::Center, &panel.center),
                (glimpse_services::Zone::Right, &panel.right),
            ]
            .into_iter()
            .flat_map(|(zone, names)| {
                names.iter().filter_map(move |name| {
                    glimpse_config::resolve_applet(name, &config.applets).and_then(|applet| {
                        match applet.kind {
                            glimpse_config::AppletKind::Exec(exec) => Some((exec.applet, zone)),
                            _ => None,
                        }
                    })
                })
            })
            .collect::<Vec<_>>()
        })
        .collect();
    let applets = if let Ok(mut registry) = attached.lock() {
        registry.set_layout(layout.clone(), configured);
        registry.set_edges(
            config
                .panels
                .iter()
                .map(|panel| crate::applets::exec::edge(panel.position))
                .collect(),
        );
        registry.applets.clone()
    } else {
        Vec::new()
    };
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
                margin: cfg.margin,
                monitor: monitor.clone(),
                left: cfg.left.clone(),
                center: cfg.center.clone(),
                right: cfg.right.clone(),
                attached: crate::attached::visible_on(&applets, &layout, index, &key.monitor),
                applets: config.applets.clone(),
                regional: config.regional.clone(),
                blur: config.appearance.blur.clone(),
                handles: services.handles.clone(),
                dialog: dialog.clone(),
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

#[cfg(test)]
mod tests;
