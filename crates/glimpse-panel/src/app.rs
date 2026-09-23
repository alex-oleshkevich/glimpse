use adw::gdk::{self, prelude::*};
use adw::prelude::{AdwDialogExt, AlertDialogExt};
use futures_util::StreamExt;
use gettextrs::gettext;
use gtk4::prelude::{GtkWindowExt, WidgetExt};
use std::{collections::HashMap, path::PathBuf};
use zeroize::Zeroizing;

use glimpse_config::{
    Config, DARK_STYLESHEET, PANEL_STYLESHEET, stylesheet, user_dark_stylesheet, user_stylesheet,
    watch_config, watch_theme,
};
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    Answer, BluetoothHandle, CommandError, NetworkHandle, SessionAction, SessionActionsHandle,
};
use glimpse_widgets::{
    NetworkEntered, PairingAnswer, PairingDialog, PairingEntry as Entry, SecretAnswer,
    SecretDialog, Sheets, Styles,
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
};
use tokio::task::JoinHandle;

use crate::{
    applet::{Report, spawn_reported},
    applets::bluetooth::{cap, typed},
    components::{self, panel},
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
    pub config_path: Option<PathBuf>,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant, clippy::enum_variant_names)]
pub enum AppInput {
    BluetoothPrompt(Option<(Entry, String, String)>),
    NetworkSecret(Option<SecretPrompt>),
    SessionConfirm(SessionDialog),
    SessionDialogClosed,
    SessionRun(SessionAction),
    ConfigChanged(Config),
    MonitorsChanged,
    ServicesReady(PanelServices),
    ThemeChanged,
}

pub struct App {
    config: Config,
    panels: Vec<PanelState>,
    theme_watch: JoinHandle<()>,
    styles: Styles,
    services: Option<PanelServices>,
    services_start: JoinHandle<()>,
    host: adw::ApplicationWindow,
    bluetooth_watch: Option<JoinHandle<()>>,
    network_watch: Option<JoinHandle<()>>,
    pairing: Option<(PairingDialog, glib::SignalHandlerId)>,
    secret: Option<(SecretDialog, glib::SignalHandlerId)>,
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
        let theme_watch = spawn_theme_watch(&init.config.appearance.theme, sender.clone());
        let services_start = spawn_services(init.config.clone(), sender.clone());
        spawn_config_watch(init.config_path, init.config.clone(), sender);

        let styles = Styles::install(color_scheme(init.config.appearance.color_scheme));
        let model = App {
            config: init.config,
            panels: Default::default(),
            theme_watch,
            styles,
            services: None,
            services_start,
            host: root.clone(),
            bluetooth_watch: None,
            network_watch: None,
            pairing: None,
            secret: None,
        };
        model.reload_styles();

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
                if let Some(services) = &self.services {
                    services.reconfigure(&self.config);
                }
                self.styles
                    .set_color_scheme(color_scheme(self.config.appearance.color_scheme));
                if renamed {
                    self.theme_watch.abort();
                    self.theme_watch =
                        spawn_theme_watch(&self.config.appearance.theme, sender.clone());
                    self.reload_styles();
                }
            }
            AppInput::MonitorsChanged => {}
            AppInput::ServicesReady(services) => {
                services.reconfigure(&self.config);
                self.bluetooth_watch = Some(spawn_bluetooth_watch(
                    services.bluetooth.clone(),
                    sender.clone(),
                ));
                self.network_watch = Some(spawn_network_watch(
                    services.network.clone(),
                    sender.clone(),
                ));
                self.services = Some(services);
            }
            AppInput::BluetoothPrompt(pairing) => {
                if let Some(services) = &self.services {
                    if pairing.is_some() {
                        self.close_popovers();
                    }
                    let bluetooth = services.bluetooth.clone();
                    self.show_pairing(pairing, &bluetooth);
                    if self.pairing.is_none() && self.host.is_visible() {
                        self.host.set_visible(false);
                    }
                }
                return;
            }
            AppInput::NetworkSecret(request) => {
                if let Some(services) = &self.services {
                    if listed(&self.config, "network") {
                        return;
                    }
                    if request.is_some() {
                        self.close_popovers();
                    }
                    let network = services.network.clone();
                    self.show_secret(request, &network);
                    if self.secret.is_none() && self.pairing.is_none() && self.host.is_visible() {
                        self.host.set_visible(false);
                    }
                }
                return;
            }
            AppInput::SessionConfirm(request) => {
                self.close_popovers();
                self.show_session_dialog(request, &sender);
                return;
            }
            AppInput::SessionDialogClosed => {
                if self.pairing.is_none() && self.secret.is_none() && self.host.is_visible() {
                    self.host.set_visible(false);
                }
                return;
            }
            AppInput::SessionRun(action) => {
                self.close_popovers();
                self.run_session_action(action);
                return;
            }
            AppInput::ThemeChanged => self.reload_styles(),
        }
        reconcile_panels(
            &mut self.panels,
            &self.config,
            self.services.as_ref(),
            sender.input_sender().clone(),
        );
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
        if let Some(services) = self.services.take() {
            relm4::spawn(services.shutdown());
        }
    }
}

impl App {
    fn close_popovers(&self) {
        for state in &self.panels {
            state.controller.emit(panel::Input::ClosePopover);
        }
    }

    fn show_pairing(
        &mut self,
        pairing: Option<(Entry, String, String)>,
        bluetooth: &BluetoothHandle,
    ) {
        let Some((entry, device, name)) = pairing else {
            close(self.pairing.take());
            return;
        };

        let dialog = match &self.pairing {
            Some((dialog, _)) => dialog.clone(),
            None => {
                let dialog = PairingDialog::new();
                let handle = bluetooth.clone();
                let answered = dialog.connect_answered(move |_, answer| {
                    let handle = handle.clone();
                    let answer = answered(answer);
                    relm4::spawn(async move {
                        let _ = handle.answer_pairing(answer).await;
                    });
                });
                self.host.set_title(Some(&gettext("Bluetooth pairing")));
                self.host.set_visible(true);
                dialog.present(Some(&self.host));
                self.pairing = Some((dialog.clone(), answered));
                dialog
            }
        };

        dialog.ask(&device, &name, entry);
    }

    /// The prompt is presented here rather than by the applet: an applet holds no state outliving
    /// its widget, and NetworkManager can ask for a secret at any moment — including while the
    /// popover is shut. A dialog on a never-mapped window is queued rather than shown, so the host
    /// is made visible while one is up.
    fn show_secret(&mut self, request: Option<SecretPrompt>, network: &NetworkHandle) {
        let Some(SecretPrompt {
            key,
            name,
            retry,
            entered,
        }) = request
        else {
            close_secret(self.secret.take());
            return;
        };

        let dialog = match &self.secret {
            Some((dialog, _)) => dialog.clone(),
            None => {
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
                    relm4::spawn(async move {
                        let _ = handle.answer_secret(answer).await;
                    });
                });
                self.host.set_title(Some(&gettext("Network password")));
                self.host.set_visible(true);
                dialog.present(Some(&self.host));
                self.secret = Some((dialog.clone(), answered));
                dialog
            }
        };

        dialog.ask(&key, &name, retry, entered);
    }

    fn show_session_dialog(&self, request: SessionDialog, sender: &ComponentSender<Self>) {
        let dialog = adw::AlertDialog::new(Some(&request.title), Some(&request.body));
        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("accept", &request.accept);
        dialog.set_response_appearance("accept", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let closed = sender.input_sender().clone();
        let action = request.action.clone();
        let actions = self
            .services
            .as_ref()
            .map(|services| services.session_actions.clone());
        let notifications = self
            .services
            .as_ref()
            .map(|services| services.notifications());
        dialog.connect_response(None, move |_, response| {
            if session_accepted(response)
                && let (Some(actions), Some(notifications)) =
                    (actions.clone(), notifications.clone())
            {
                execute_session(actions, notifications, action.clone());
            }
            let _ = closed.send(AppInput::SessionDialogClosed);
        });
        self.host.set_title(Some(&request.title));
        self.host.set_visible(true);
        dialog.present(Some(&self.host));
    }

    fn run_session_action(&self, action: SessionAction) {
        let Some(services) = &self.services else {
            return;
        };
        execute_session(
            services.session_actions.clone(),
            services.notifications(),
            action,
        );
    }

    fn reload_styles(&self) {
        let appearance = &self.config.appearance;
        self.styles.load(&Sheets {
            theme: stylesheet(&appearance.theme, PANEL_STYLESHEET),
            theme_dark: stylesheet(&appearance.theme, DARK_STYLESHEET),
            dropin: user_stylesheet(),
            dropin_dark: user_dark_stylesheet(),
        });
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

fn close<D: IsA<adw::Dialog>>(open: Option<(D, glib::SignalHandlerId)>) {
    let Some((dialog, handler)) = open else {
        return;
    };
    dialog.block_signal(&handler);
    dialog.as_ref().force_close();
    dialog.unblock_signal(&handler);
}

fn color_scheme(scheme: glimpse_config::ColorScheme) -> adw::ColorScheme {
    match scheme {
        glimpse_config::ColorScheme::Light => adw::ColorScheme::ForceLight,
        glimpse_config::ColorScheme::Dark => adw::ColorScheme::ForceDark,
        glimpse_config::ColorScheme::Auto => adw::ColorScheme::Default,
    }
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
        PairingAnswer::Pin(pin) => Answer::Pin(pin),
        PairingAnswer::Passkey(passkey) => Answer::Passkey(passkey),
    }
}

fn close_secret(open: Option<(SecretDialog, glib::SignalHandlerId)>) {
    let Some((dialog, handler)) = open else {
        return;
    };
    dialog.block_signal(&handler);
    adw::prelude::AdwDialogExt::force_close(&dialog);
    dialog.unblock_signal(&handler);
}

/// A secret request reaches the host as its key, the network's name and whether NetworkManager
/// rejected the last answer.
/// Whether a panel carries the applet by that name. The network applet asks for a secret inside
/// its own popover, so the host's dialog is for the configuration that has no such applet and
/// would otherwise leave NetworkManager waiting on a prompt nothing can show.
fn listed(config: &Config, applet: &str) -> bool {
    config.panels.iter().any(|panel| {
        [&panel.left, &panel.center, &panel.right]
            .into_iter()
            .flatten()
            .any(|name| name == applet)
    })
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
                state.pairing.as_ref().and_then(|prompt| {
                    let name = state
                        .name(prompt.device())
                        .filter(|name| !name.is_empty())
                        .map_or_else(|| gettext("this device"), cap);
                    typed(prompt).map(|entry| (entry, prompt.device().as_str().to_owned(), name))
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    pub index: usize,
    pub monitor: String,
}

struct PanelState {
    pub key: Key,
    pub controller: Controller<components::panel::Panel>,
}

fn reconcile_panels(
    panels: &mut Vec<PanelState>,
    config: &Config,
    services: Option<&PanelServices>,
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
                left: cfg.left.clone(),
                center: cfg.center.clone(),
                right: cfg.right.clone(),
                applets: config.applets.clone(),
                regional: config.regional.clone(),
                blur: config.appearance.blur.clone(),
                compositor: services.compositor.clone(),
                keyboard: services.keyboard.clone(),
                calendar: services.calendar.clone(),
                mpris: services.mpris.clone(),
                heartbeat: services.heartbeat.clone(),
                tray: services.tray.clone(),
                bluetooth: services.bluetooth.clone(),
                network: services.network.clone(),
                audio: services.audio.clone(),
                brightness: services.brightness.clone(),
                night_light: services.night_light(),
                color_picker: services.color_picker.clone(),
                notifications: services.notifications(),
                weather: services.weather(),
                idle: services.idle(),
                session_actions: services.session_actions.clone(),
                clipboard: services.clipboard.clone(),
                battery: services.battery.clone(),
                places: services.places.clone(),
                printing: services.printing.clone(),
                removable: services.removable.clone(),
                privacy: services.privacy.clone(),
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
mod tests {
    use super::*;

    #[test]
    fn session_dialog_accepts_only_the_destructive_response() {
        assert!(!session_accepted("cancel"));
        assert!(!session_accepted("close"));
        assert!(session_accepted("accept"));
    }
}
