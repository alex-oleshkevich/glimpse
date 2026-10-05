use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gettextrs::gettext;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use glimpse_config::{Applet as AppletConfig, AppletKind};
use glimpse_dbus::network_manager as nm;
use glimpse_dbus::notifications::NotificationsProviderHandle;
use glimpse_services::{
    NetworkError, NetworkHandle, NetworkId, NetworkSecret, NetworkState, SecretAnswer,
    SecretRequest,
};
use glimpse_widgets::{IndicatorSpec, NetworkAsk, NetworkEntered, NetworkPopover};

use crate::applet::popover::{PopoverHandle, Seat};
use crate::applet::{Applet, Ctx, Input, Opener, Report, spawn_command, spawn_reported};

use super::render;

const IDLE: &str = "network-wireless-symbolic";

/// What the popover's prompt page is asking for. A hidden network asks twice — its name, then its
/// password — so the name is carried into the second ask.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Asking {
    Name,
    Hidden { ssid: String },
    Joining { id: NetworkId, name: String },
    Agent(SecretRequest),
}

const HIDDEN_SECURITY: [nm::Security; 4] = [
    nm::Security::Open,
    nm::Security::Wep,
    nm::Security::Wpa2,
    nm::Security::Wpa3,
];

fn hidden_choices() -> Vec<String> {
    vec![
        gettext("None"),
        gettext("WEP"),
        gettext("WPA & WPA2 Personal"),
        gettext("WPA3 Personal"),
    ]
}

/// What a secret asked for under this setting name is allowed to look like. A VPN plugin's token
/// obeys no Wi-Fi rule, so only a wireless secret is held to the passphrase lengths.
pub fn entered_for(setting: &str) -> NetworkEntered {
    match setting {
        "802-11-wireless-security" => NetworkEntered::Passphrase,
        _ => NetworkEntered::Secret,
    }
}

/// Whether a prompt outlives the popover that was showing it. NetworkManager is waiting on the one
/// it raised itself and will not ask twice; a question the user started is theirs to abandon, and
/// keeping it resumes a stale password page for whatever network was selected last.
fn survives_reopen(held: Option<&Asking>) -> bool {
    matches!(held, Some(Asking::Agent(_)))
}

fn agent_key(request: &SecretRequest) -> String {
    format!("{}:{}", request.setting, request.name)
}

/// Which command a row's own id belongs to. A row in *Known networks* or *VPN* carries a settings
/// profile path, which `connect_access_point` cannot find — it searches access points alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    AccessPoint,
    Profile,
    Vpn,
    Wired,
}

fn route(state: &NetworkState, id: &NetworkId) -> Route {
    if state.vpn.iter().any(|one| &one.id == id) {
        return Route::Vpn;
    }
    if state.wired.iter().any(|one| &one.id == id) {
        return Route::Wired;
    }
    match state.known.iter().any(|one| &one.id == id) {
        true => Route::Profile,
        false => Route::AccessPoint,
    }
}

/// The name to ask a password for before requesting activation, or `None` when nothing needs
/// asking. NetworkManager tears the working connection down the moment an activation is requested,
/// so a network that would then ask for a password is asked about first.
fn asks_first(state: &NetworkState, id: &NetworkId) -> Option<String> {
    let access = state.networks.iter().find(|one| &one.id == id)?;
    let unsaved = access.saved.is_none();
    (unsaved && access.security.needs_a_secret() && !access.security.is_enterprise())
        .then(|| render::name_of(access))
}

fn tell<F, T>(
    notifications: &NotificationsProviderHandle,
    operation: &'static str,
    summary: String,
    future: F,
) where
    F: std::future::Future<Output = Result<T, NetworkError>> + Send + 'static,
    T: Send + 'static,
{
    let report = Report {
        notifications: notifications.clone(),
        app_name: gettext("Network"),
        icon: IDLE.to_owned(),
        summary,
    };
    spawn_reported(
        operation,
        report,
        |error| error.failure().and_then(render::wording),
        future,
    );
}

pub struct Network {
    state: NetworkState,
    network: NetworkHandle,
    notifications: NotificationsProviderHandle,
    tooltip_format: Option<String>,
    footer: Option<(String, Vec<String>)>,
    vpn_chip: bool,
    metered_in_tooltip: bool,
    visible_networks: usize,
    spec: Vec<IndicatorSpec>,
    expanded: Rc<Cell<bool>>,
    all: Rc<Cell<bool>>,
    asking: Rc<RefCell<Option<Asking>>>,
    opener: Option<Opener>,
    shown: glib::WeakRef<NetworkPopover>,
}

fn themed(name: &str) -> gio::Icon {
    gio::ThemedIcon::new(name).upcast()
}

impl Network {
    pub fn start(network: NetworkHandle, notifications: NotificationsProviderHandle) -> Self {
        let state = network.snapshot();
        let mut applet = Self {
            state,
            network,
            notifications,
            tooltip_format: None,
            footer: None,
            vpn_chip: true,
            metered_in_tooltip: true,
            visible_networks: 8,
            spec: Vec::new(),
            expanded: Rc::new(Cell::new(false)),
            all: Rc::new(Cell::new(false)),
            asking: Rc::new(RefCell::new(None)),
            opener: None,
            shown: glib::WeakRef::new(),
        };
        applet.refresh();
        applet
    }

    /// A request from NetworkManager owns the prompt while it is up: it is the only one that can
    /// arrive without the user having asked for anything, so it opens the popover to be answered.
    fn raise(&mut self, ctx: &Ctx) {
        let held = self.asking.borrow().clone();
        let asked = match (&self.state.secret, held) {
            (Some(request), Some(Asking::Agent(held)))
                if agent_key(&held) == agent_key(request) =>
            {
                return;
            }
            (Some(request), _) => Some(Asking::Agent(request.clone())),
            (None, Some(Asking::Agent(_))) => None,
            (None, _) => return,
        };
        let raising = asked.is_some();
        self.asking.replace(asked);
        if raising {
            ctx.opener().open_popover();
        }
    }

    fn refresh(&mut self) {
        self.spec = self.indicator();
        if let Some(shown) = self.shown.upgrade() {
            self.dress(&shown);
        }
    }

    fn dress(&self, shown: &NetworkPopover) {
        let ask = self.ask();
        shown.set_prompt(ask.as_ref());
        let radio = self.state.wifi;
        shown.set_radio(
            &gettext("Wi-Fi"),
            &render::status(&self.state),
            self.state.icon_name().unwrap_or(IDLE),
            radio.is_some_and(|one| one.enabled),
            radio.is_some_and(|one| !one.blocked()),
        );

        let (entries, others) = render::entries(
            &self.state,
            self.visible_networks,
            self.expanded.get(),
            self.all.get(),
        );
        shown.set_entries(&entries);
        shown.set_others(others.count, others.open, others.rest.as_deref());
        shown.set_scanning(self.state.scanning);
        shown.set_hidden_entry(self.state.wifi.is_some_and(|one| one.enabled));

        let details: Vec<_> = entries
            .iter()
            .filter_map(|entry| render::details(&self.state, &entry.id))
            .collect();
        shown.set_details(&details);

        shown.set_footer(self.footer.as_ref().map(|(label, _)| label.as_str()));

        if let Some(opener) = &self.opener {
            opener.typing(ask.is_some());
        }
    }

    fn joining_entry(&self, id: &NetworkId) -> NetworkEntered {
        let point = self
            .state
            .networks
            .iter()
            .find(|network| &network.id == id)
            .map(|network| network.security);
        match point {
            Some(nm::Security::Wep) => NetworkEntered::Secret,
            _ => NetworkEntered::Passphrase,
        }
    }

    fn ask(&self) -> Option<NetworkAsk> {
        let asking = self.asking.borrow().clone();
        match asking? {
            Asking::Name => Some(NetworkAsk {
                key: "hidden".to_owned(),
                network: gettext("Hidden network"),
                question: gettext("Type the name the network broadcasts nothing about."),
                entered: NetworkEntered::Name,
                accept: gettext("Continue"),
                choices: Vec::new(),
                open_choice: None,
            }),
            Asking::Hidden { ssid } => Some(NetworkAsk {
                key: format!("hidden-secret:{ssid}"),
                network: render::cap(&ssid),
                question: gettext("Choose how the network is secured, then type its password."),
                entered: NetworkEntered::Secret,
                accept: gettext("Connect"),
                choices: hidden_choices(),
                open_choice: Some(0),
            }),
            Asking::Joining { id, name } => Some(NetworkAsk {
                key: format!("secret:{name}"),
                network: render::cap(&name),
                question: gettext("The network needs a password before this computer can join it."),
                entered: self.joining_entry(&id),
                accept: gettext("Connect"),
                choices: Vec::new(),
                open_choice: None,
            }),
            Asking::Agent(request) => Some(NetworkAsk {
                key: agent_key(&request),
                choices: Vec::new(),
                open_choice: None,
                network: render::cap(&request.name),
                question: match request.retry {
                    true => gettext("{network} refused that password. Check it and try again.")
                        .replace("{network}", &render::cap(&request.name)),
                    false => {
                        gettext("The network needs a password before this computer can join it.")
                    }
                },
                entered: entered_for(&request.setting),
                accept: match request.retry {
                    true => gettext("Try again"),
                    false => gettext("Connect"),
                },
            }),
        }
    }

    fn indicator(&self) -> Vec<IndicatorSpec> {
        let Some(icon) = self.state.icon_name() else {
            return Vec::new();
        };
        let tooltip = render::tooltip(
            &self.state,
            self.tooltip_format.as_deref(),
            self.metered_in_tooltip,
        );

        let mut chips = vec![IndicatorSpec {
            icon: Some(themed(icon)),
            tooltip: tooltip.clone(),
            ..Default::default()
        }];

        if let Some(vpn) = self.state.vpn_icon_name(self.vpn_chip) {
            chips.push(IndicatorSpec {
                icon: Some(themed(vpn)),
                tooltip: Some(gettext("VPN is connected")),
                ..Default::default()
            });
        }
        chips
    }
}

impl Applet for Network {
    fn configure(&mut self, ctx: &Ctx, config: &AppletConfig) {
        self.opener = Some(ctx.opener());
        let AppletKind::Network(cfg) = &config.kind else {
            return;
        };
        self.visible_networks = cfg.visible_networks;
        self.vpn_chip = cfg.vpn_chip;
        self.metered_in_tooltip = cfg.metered_in_tooltip;
        self.tooltip_format = config.common.tooltip_format.clone();
        self.footer = config
            .common
            .settings()
            .map(|(label, command)| (label.to_owned(), command.to_vec()));
        self.refresh();
    }

    fn handle(&mut self, ctx: &Ctx, input: &Input) {
        match input {
            Input::Woken => {
                self.state = self.network.snapshot();
                self.raise(ctx);
            }
            Input::Tick | Input::Pointer(_) => return,
        }
        self.refresh();
    }

    fn indicators(&self) -> Vec<IndicatorSpec> {
        self.spec.clone()
    }

    fn popover(&mut self, seat: &Seat) -> Option<Box<dyn PopoverHandle>> {
        let shown = NetworkPopover::new();

        shown.connect_wifi_toggled({
            let network = self.network.clone();
            let notifications = self.notifications.clone();
            move |_, on| {
                let network = network.clone();
                tell(
                    &notifications,
                    "network.set_wifi_enabled",
                    gettext("Could not switch Wi-Fi"),
                    async move { network.set_wifi_enabled(on).await },
                );
            }
        });

        shown.connect_activated({
            let network = self.network.clone();
            let notifications = self.notifications.clone();
            let asking = Rc::clone(&self.asking);
            let opener = seat.opener();
            move |_, id| {
                let id = NetworkId::new(id);
                let opener = opener.clone();
                let state = network.snapshot();
                let route = route(&state, &id);
                if let Some(name) = asks_first(&state, &id) {
                    asking.replace(Some(Asking::Joining { id, name }));
                    opener.wake();
                    return;
                }
                let network = network.clone();
                match route {
                    Route::Vpn => tell(
                        &notifications,
                        "network.connect_vpn",
                        gettext("Could not connect the VPN"),
                        async move { network.connect_vpn(id).await },
                    ),
                    Route::Profile => tell(
                        &notifications,
                        "network.connect_profile",
                        gettext("Could not connect"),
                        async move { network.connect_profile(id).await },
                    ),
                    Route::Wired => tell(
                        &notifications,
                        "network.connect_device",
                        gettext("Could not connect"),
                        async move { network.connect_device(id).await },
                    ),
                    Route::AccessPoint => tell(
                        &notifications,
                        "network.connect_access_point",
                        gettext("Could not connect"),
                        async move { network.connect_access_point(id, None).await },
                    ),
                }
                opener.wake();
            }
        });

        shown.connect_acted({
            let network = self.network.clone();
            let notifications = self.notifications.clone();
            let opener = seat.opener();
            move |_, id, action| {
                let id = NetworkId::new(id);
                let network = network.clone();
                match action {
                    "disconnect" => tell(
                        &notifications,
                        "network.disconnect",
                        gettext("Could not disconnect"),
                        async move { network.disconnect(id).await },
                    ),
                    "forget" => tell(
                        &notifications,
                        "network.forget",
                        gettext("Could not forget that network"),
                        async move { network.forget(id).await },
                    ),
                    "disconnect-vpn" => tell(
                        &notifications,
                        "network.disconnect_vpn",
                        gettext("Could not disconnect the VPN"),
                        async move { network.disconnect_vpn(id).await },
                    ),
                    _ => {}
                }
                opener.wake();
            }
        });

        shown.connect_toggled({
            let network = self.network.clone();
            let notifications = self.notifications.clone();
            move |_, id, action, on| {
                if action != "autoconnect" {
                    return;
                }
                let id = NetworkId::new(id);
                let network = network.clone();
                tell(
                    &notifications,
                    "network.set_autoconnect",
                    gettext("Could not change that setting"),
                    async move { network.set_autoconnect(id, on).await },
                );
            }
        });

        shown.connect_hidden_network({
            let asking = Rc::clone(&self.asking);
            let opener = seat.opener();
            move |_| {
                asking.replace(Some(Asking::Name));
                opener.wake();
            }
        });

        shown.connect_answered({
            let network = self.network.clone();
            let notifications = self.notifications.clone();
            let asking = Rc::clone(&self.asking);
            let opener = seat.opener();
            move |shown, accepted, entered| {
                let chosen = shown.chosen();
                let held = asking.replace(None);
                let network = network.clone();
                match (held, accepted) {
                    (Some(Asking::Name), true) => {
                        asking.replace(Some(Asking::Hidden {
                            ssid: entered.to_owned(),
                        }));
                    }
                    (Some(Asking::Hidden { ssid }), true) => {
                        let security = HIDDEN_SECURITY
                            .get(chosen as usize)
                            .copied()
                            .unwrap_or(nm::Security::Wpa2);
                        let secret = (security != nm::Security::Open)
                            .then(|| NetworkSecret::new(entered.to_owned()));
                        tell(
                            &notifications,
                            "network.connect_hidden",
                            gettext("Could not connect"),
                            async move { network.connect_hidden(ssid, security, secret).await },
                        );
                    }
                    (Some(Asking::Joining { id, .. }), true) => {
                        let secret = NetworkSecret::new(entered.to_owned());
                        tell(
                            &notifications,
                            "network.connect_access_point",
                            gettext("Could not connect"),
                            async move { network.connect_access_point(id, Some(secret)).await },
                        );
                    }
                    (Some(Asking::Agent(_)), accepted) => {
                        let answer = match accepted {
                            true => SecretAnswer::Secret(NetworkSecret::new(entered.to_owned())),
                            false => SecretAnswer::Refused,
                        };
                        tell(
                            &notifications,
                            "network.answer_secret",
                            gettext("Could not send that password"),
                            async move { network.answer_secret(answer).await },
                        );
                    }
                    _ => {}
                }
                opener.wake();
            }
        });

        shown.connect_expanded({
            let expanded = Rc::clone(&self.expanded);
            let opener = seat.opener();
            move |_| {
                expanded.set(!expanded.get());
                opener.wake();
            }
        });

        shown.connect_show_all({
            let all = Rc::clone(&self.all);
            let opener = seat.opener();
            move |_| {
                all.set(true);
                opener.wake();
            }
        });

        shown.connect_footer_activated({
            let footer = self.footer.clone();
            move |_| {
                if let Some((_, command)) = &footer {
                    crate::applet::popover::run(command);
                }
            }
        });

        self.expanded.set(false);
        self.all.set(false);
        if !survives_reopen(self.asking.borrow().as_ref()) {
            self.asking.replace(None);
        }
        self.shown.set(Some(&shown));
        self.dress(&shown);

        shown.connect_map({
            let network = self.network.clone();
            move |_| {
                let network = network.clone();
                spawn_command(
                    "network.start_scan",
                    async move { network.start_scan().await },
                );
            }
        });
        let closing = self.network.clone();
        shown.connect_unmap(move |_| {
            let closing = closing.clone();
            spawn_command(
                "network.stop_scan",
                async move { closing.stop_scan().await },
            );
        });

        Some(Box::new(shown))
    }
}

#[cfg(test)]
mod tests;
