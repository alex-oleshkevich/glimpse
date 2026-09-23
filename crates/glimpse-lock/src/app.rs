use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use glimpse_config::{
    Config, DARK_STYLESHEET, LOCK_STYLESHEET, Remember, stylesheet, user_dark_stylesheet,
    user_stylesheet, watch_config, watch_theme,
};
use glimpse_dbus::Buses;
use glimpse_dbus::accounts::{AccountsProxy, AccountsUserProxy};
use glimpse_dbus::notifications::{
    NotificationRecord, NotificationsProvider, NotificationsProviderHandle,
    NotificationsProviderState,
};
use glimpse_dbus::weather::{WeatherProvider, WeatherProviderState};
use glimpse_services::{
    Battery, BatteryState, Bluetooth, BluetoothDependencies, BluetoothState, Compositor,
    CompositorState, Keyboard, KeyboardDependencies, KeyboardLayouts, Mpris, MprisHandle,
    MprisPlayers, Network, NetworkDependencies, NetworkState, PlayerAction, Running,
};
use glimpse_widgets::SessionActionState;
use glimpse_widgets::{Sheets, Styles};
use glimpse_widgets::TransportAction;
use glimpse_widgets::raster::{self, Raster};
use gtk4::prelude::{
    ApplicationExt, Cast, CastNone, DisplayExt, GtkWindowExt, ListModelExt, MonitorExt, WidgetExt,
};
use gtk4::{gdk, glib};
use relm4::{ComponentParts, ComponentSender, SimpleComponent};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::auth::{self, Message, Verdict};
use crate::background::{self, Cache, Key};
use crate::chips;
use crate::lifecycle::{Effect, Input, Lifecycle, Notice, Refusal, Settings, sleep_wait};
use crate::logind::{self, Request};
use crate::media;
use crate::notify;
use crate::session::{self, Action};
use crate::status::{self, Status};
use crate::surface::{Look, PromptState, Surfaces};
use crate::user;

const ACCOUNTS_TIMEOUT: Duration = Duration::from_secs(5);

pub struct AppInit {
    pub config: Config,
    pub pam_service: auth::Service,
    pub config_path: Option<PathBuf>,
    pub standalone: bool,
    pub cant_verify: bool,
    pub user: Option<String>,
    pub unlocked: Arc<AtomicBool>,
}

#[allow(clippy::large_enum_variant)]
pub enum AppInput {
    ConfigChanged(Config),
    ThemeChanged,
    Start {
        locked_hint: bool,
        sleep_wait: Duration,
    },
    Lifecycle(Input),
    FocusedOutput(Option<String>),
    Outputs,
    Decoded(Key, Option<Raster>),
    SchemeChanged,
    DisplayName(Option<String>),
    SessionAnswers(session::Answers),
    SessionRequested(Action, u64),
    SessionResult(Action, u64, session::Outcome),
    BlockInhibitedChanged,
    ServicesReady {
        mpris_service: Running<Mpris>,
        mpris: MprisHandle,
        notifications_service: NotificationsProvider,
        notifications: NotificationsProviderHandle,
        status_services: StatusServices,
    },
    Media(MprisPlayers),
    Notifications(NotificationsProviderState),
    TrackAction(TransportAction),
    Battery(BatteryState),
    Network(NetworkState),
    Bluetooth(BluetoothState),
    Layouts(KeyboardLayouts),
    Weather(WeatherProviderState),
}

impl std::fmt::Debug for AppInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::ConfigChanged(_) => "ConfigChanged",
            Self::ThemeChanged => "ThemeChanged",
            Self::Start { .. } => "Start",
            Self::Lifecycle(_) => "Lifecycle",
            Self::FocusedOutput(_) => "FocusedOutput",
            Self::Outputs => "Outputs",
            Self::Decoded(..) => "Decoded",
            Self::SchemeChanged => "SchemeChanged",
            Self::DisplayName(_) => "DisplayName",
            Self::SessionAnswers(_) => "SessionAnswers",
            Self::SessionRequested(..) => "SessionRequested",
            Self::SessionResult(..) => "SessionResult",
            Self::BlockInhibitedChanged => "BlockInhibitedChanged",
            Self::ServicesReady { .. } => "ServicesReady",
            Self::Media(_) => "Media",
            Self::Notifications(_) => "Notifications",
            Self::TrackAction(_) => "TrackAction",
            Self::Battery(_) => "Battery",
            Self::Network(_) => "Network",
            Self::Bluetooth(_) => "Bluetooth",
            Self::Layouts(_) => "Layouts",
            Self::Weather(_) => "Weather",
        };
        f.write_str(name)
    }
}

pub struct App {
    config: Config,
    theme_watch: JoinHandle<()>,
    styles: Styles,
    machine: Lifecycle,
    user: Option<String>,
    pam_service: auth::Service,
    unlocked: Arc<AtomicBool>,
    logind: Option<UnboundedSender<Request>>,
    notices: Option<UnboundedSender<Notice>>,
    focused: Option<String>,
    sleep_wait: Duration,
    surfaces: Option<Surfaces>,
    backgrounds: Cache<gdk::Texture>,
    color: gdk::RGBA,
    display_name: Option<String>,
    session_answers: session::Answers,
    session: HashMap<&'static str, SessionActionState>,
    session_request_ids: Rc<Cell<u64>>,
    services_start: JoinHandle<()>,
    mpris_service: Option<Running<Mpris>>,
    mpris: Option<MprisHandle>,
    mpris_watch: Option<JoinHandle<()>>,
    mpris_players: MprisPlayers,
    notifications_service: Option<NotificationsProvider>,
    notifications_watch: Option<JoinHandle<()>>,
    notif_records: Vec<NotificationRecord>,
    notif_unavailable: bool,
    lock_started: Option<DateTime<Utc>>,
    status_services: Option<StatusServices>,
    status: Status,
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
        let theme_watch = spawn_theme_watch(&init.config.appearance.theme, sender.clone());
        spawn_config_watch(init.config_path, init.config.clone(), sender.clone());
        spawn_display_name(sender.clone());
        watch_outputs(&sender);
        adw::StyleManager::default().connect_dark_notify({
            let sender = sender.clone();
            move |_| sender.input(AppInput::SchemeChanged)
        });

        let refusal = if !gtk4_session_lock::is_supported() {
            tracing::error!("the compositor has no ext-session-lock-v1; locking is impossible");
            Some(Refusal::NoSessionLock)
        } else if init.cant_verify {
            Some(Refusal::CantVerify)
        } else {
            None
        };
        let machine = Lifecycle::new(Settings {
            standalone: init.standalone,
            refusal,
            lock_on_request: init.config.power.lock_on_request,
            lock_before_sleep: init.config.power.lock_before_sleep,
        });

        let (logind, notices) = if init.standalone {
            tracing::warn!("standalone: logind is not consulted and nothing is notified");
            sender.input(AppInput::Start {
                locked_hint: false,
                sleep_wait: sleep_wait(None),
            });
            (None, None)
        } else {
            let events = sender.clone();
            let logind = logind::spawn(move |event| {
                events.input(match event {
                    logind::Event::Start {
                        locked_hint,
                        sleep_wait,
                    } => AppInput::Start {
                        locked_hint,
                        sleep_wait,
                    },
                    logind::Event::Lifecycle(input) => AppInput::Lifecycle(input),
                    logind::Event::SessionAnswers(answers) => AppInput::SessionAnswers(answers),
                    logind::Event::SessionActionResult(action, id, outcome) => {
                        AppInput::SessionResult(action, id, outcome)
                    }
                    logind::Event::BlockInhibitedChanged => AppInput::BlockInhibitedChanged,
                });
            });
            (Some(logind), Some(notify::spawn()))
        };

        let services_start = spawn_services(init.config.clone(), sender.clone());

        let styles = Styles::install(color_scheme(init.config.appearance.color_scheme));
        let color = raster::color(&init.config.lock.background.color);
        let mut model = App {
            config: init.config,
            theme_watch,
            styles,
            machine,
            user: init.user,
            pam_service: init.pam_service,
            unlocked: init.unlocked,
            logind,
            notices,
            focused: None,
            sleep_wait: sleep_wait(None),
            surfaces: None,
            backgrounds: Cache::default(),
            color,
            display_name: None,
            session_answers: session::Answers::default(),
            session: HashMap::new(),
            session_request_ids: Rc::default(),
            services_start,
            mpris_service: None,
            mpris: None,
            mpris_watch: None,
            mpris_players: MprisPlayers::default(),
            notifications_service: None,
            notifications_watch: None,
            notif_records: Vec::new(),
            notif_unavailable: true,
            lock_started: None,
            status_services: None,
            status: Status::default(),
        };
        model.reload_styles();
        model
            .backgrounds
            .configure(background::variants(&model.config));
        model.backgrounds.set_outputs(outputs());
        model.decode(&sender);

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
                self.pam_service.reload(&config.lock.pam_service);
                let redecode = background::redecodes(&self.config, &config);
                let recolor = self.config.lock.background.color != config.lock.background.color;
                let session_changed = self.config.lock.session != config.lock.session;
                self.config = config;
                if recolor {
                    self.color = raster::color(&self.config.lock.background.color);
                }
                if redecode {
                    self.backgrounds
                        .configure(background::variants(&self.config));
                    self.decode(&sender);
                }
                if session_changed {
                    self.session =
                        session::states(&self.config.lock.session, &self.session_answers);
                }
                self.styles
                    .set_color_scheme(color_scheme(self.config.appearance.color_scheme));
                self.styles
                    .set_variant(&self.config.appearance.theme_variant);
                self.styles
                    .set_animation_speed(self.config.appearance.animation_speed);
                if let Some(mpris_service) = &self.mpris_service {
                    mpris_service.reconfigure(&hosted(&self.config));
                }
                if let Some(status_services) = &self.status_services {
                    status_services.reconfigure(&self.config);
                }
                self.restage();
                if renamed {
                    self.theme_watch.abort();
                    self.theme_watch =
                        spawn_theme_watch(&self.config.appearance.theme, sender.clone());
                    self.reload_styles();
                }
                let power = &self.config.power;
                self.step(
                    Input::Configure {
                        lock_on_request: power.lock_on_request,
                        lock_before_sleep: power.lock_before_sleep,
                    },
                    &sender,
                );
            }
            AppInput::ThemeChanged => self.reload_styles(),
            AppInput::Start {
                locked_hint,
                sleep_wait,
            } => {
                self.sleep_wait = sleep_wait;
                self.step(Input::Start { locked_hint }, &sender);
            }
            AppInput::Lifecycle(input) => {
                let resumed = matches!(input, Input::PrepareForSleep(false));
                self.step(input, &sender);
                if resumed && let Some(surfaces) = &self.surfaces {
                    surfaces.resumed();
                }
            }
            AppInput::FocusedOutput(output) => {
                if let Some(surfaces) = &self.surfaces {
                    surfaces.set_focused(output.as_deref());
                }
                self.focused = output;
            }
            AppInput::Outputs => {
                self.backgrounds.set_outputs(outputs());
                self.decode(&sender);
                self.restage();
                if let Some(surfaces) = &self.surfaces {
                    surfaces.place_prompt();
                }
            }
            AppInput::Decoded(key, raster) => {
                let failed = raster.is_none().then(|| (key.connector.clone(), key.dark));
                let texture = raster.as_ref().map(raster::texture);
                if !self.backgrounds.land(key, texture) {
                    return;
                }
                if let Some((output, dark)) = failed {
                    tracing::warn!(
                        output,
                        dark,
                        "the lock background did not resolve or decode; using its color"
                    );
                }
                self.restage();
            }
            AppInput::SchemeChanged => self.restage(),
            AppInput::DisplayName(name) => {
                self.display_name = name;
                self.restage();
            }
            AppInput::SessionAnswers(answers) => {
                self.session_answers = answers;
                self.session = session::states(&self.config.lock.session, &self.session_answers);
                self.restage();
            }
            AppInput::SessionRequested(action, id) => {
                self.logind(Request::PerformSessionAction(action, id));
            }
            AppInput::SessionResult(action, id, outcome) => match &self.surfaces {
                Some(surfaces) => surfaces.session_action_result(id, action, outcome),
                None => tracing::warn!(
                    id,
                    action = action.key(),
                    ?outcome,
                    "a session action result arrived with no lock surfaces"
                ),
            },
            AppInput::BlockInhibitedChanged => {
                if self.surfaces.is_some() {
                    self.logind(Request::FetchSessionActions);
                }
            }
            AppInput::ServicesReady {
                mpris_service,
                mpris,
                notifications_service,
                notifications,
                status_services,
            } => {
                mpris_service.reconfigure(&hosted(&self.config));
                status_services.reconfigure(&self.config);
                self.status_services = Some(status_services);
                self.mpris_watch = Some(forward(mpris.subscribe(), &sender, AppInput::Media));
                self.notifications_watch = Some(forward(
                    notifications.subscribe(),
                    &sender,
                    AppInput::Notifications,
                ));
                self.mpris_service = Some(mpris_service);
                self.mpris = Some(mpris);
                self.notifications_service = Some(notifications_service);
            }
            AppInput::Media(players) => {
                self.mpris_players = players;
                self.restage();
            }
            AppInput::Notifications(state) => {
                self.notif_records = state.view.map_or_else(Vec::new, |view| view.notifications);
                self.notif_unavailable = state.unavailable.is_some();
                self.restage();
            }
            AppInput::TrackAction(action) => self.control_track(action),
            AppInput::Battery(state) => {
                if status::replace(&mut self.status.battery, status::battery(&state)) {
                    self.restage();
                }
            }
            AppInput::Network(state) => {
                if status::replace(&mut self.status.network, status::network(&state)) {
                    self.restage();
                }
            }
            AppInput::Bluetooth(state) => {
                if status::replace(&mut self.status.bluetooth, status::bluetooth(&state)) {
                    self.restage();
                }
            }
            AppInput::Layouts(layouts) => {
                if status::replace(&mut self.status.layout, status::layout(&layouts)) {
                    self.restage();
                }
            }
            AppInput::Weather(state) => {
                if status::replace(&mut self.status.weather, status::weather(&state)) {
                    self.restage();
                }
            }
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        self.services_start.abort();
        if let Some(watch) = self.mpris_watch.take() {
            watch.abort();
        }
        if let Some(watch) = self.notifications_watch.take() {
            watch.abort();
        }
        self.mpris_service = None;
        self.notifications_service = None;
        self.status_services = None;
    }
}

impl App {
    fn step(&mut self, input: Input, sender: &ComponentSender<Self>) {
        for effect in self.machine.handle(input) {
            self.apply(effect, sender);
        }
        if self.machine.is_idle() {
            self.surfaces = None;
            self.lock_started = None;
        }
        self.render();
    }

    fn apply(&mut self, effect: Effect, sender: &ComponentSender<Self>) {
        match effect {
            Effect::Lock(generation) => {
                tracing::info!(generation, "locking");
                self.session_answers = session::Answers::default();
                self.session = HashMap::new();
                self.lock_started = lock_started_on_lock(self.lock_started, Utc::now());
                let report = sender.clone();
                let session_report = sender.clone();
                let track_report = sender.clone();
                self.surfaces = Some(Surfaces::lock(
                    generation,
                    self.look(),
                    self.focused.as_deref(),
                    self.session_request_ids.clone(),
                    move |input| report.input(AppInput::Lifecycle(input)),
                    move |action, id| session_report.input(AppInput::SessionRequested(action, id)),
                    move |action| track_report.input(AppInput::TrackAction(action)),
                ));
                spawn_display_name(sender.clone());
                self.logind(Request::FetchSessionActions);
            }
            Effect::Unlock => {
                tracing::info!("authenticated; unlocking");
                self.lock_started = None;
                if let Some(surfaces) = &self.surfaces {
                    surfaces.unlock();
                }
            }
            Effect::SetLockedHint(locked) => self.logind(Request::SetLockedHint(locked)),
            Effect::TakeInhibitor => self.logind(Request::TakeInhibitor),
            Effect::ReleaseInhibitor => self.logind(Request::ReleaseInhibitor),
            Effect::ArmSleepDeadline(token) => {
                let sender = sender.clone();
                glib::timeout_add_local_once(self.sleep_wait, move || {
                    sender.input(AppInput::Lifecycle(Input::SleepDeadline(token)));
                });
            }
            Effect::StartAttempt(attempt) => self.start_attempt(attempt, sender),
            Effect::DiscardSubmit => {
                if let Some(surfaces) = &self.surfaces {
                    surfaces.discard_submit();
                }
            }
            Effect::Shake => {
                if let Some(surfaces) = &self.surfaces {
                    surfaces.shake();
                }
            }
            Effect::Notify(notice) => match &self.notices {
                Some(notices) => {
                    let _ = notices.send(notice);
                }
                None => {
                    let (summary, body) = notify::text(notice);
                    tracing::warn!(summary, body, "notice");
                }
            },
            Effect::Exit(unlocked) => {
                self.unlocked.store(unlocked, Ordering::SeqCst);
                relm4::main_application().quit();
            }
        }
    }

    fn start_attempt(&self, attempt: u64, sender: &ComponentSender<Self>) {
        let finished = move |verdict| AppInput::Lifecycle(Input::AuthFinished { attempt, verdict });
        let failed = Verdict::Refused {
            message: Message::Failed,
            shake: false,
        };
        let (Some(surfaces), Some(user)) = (&self.surfaces, &self.user) else {
            tracing::warn!("no lock surfaces or no username; not attempting");
            sender.input(finished(failed));
            return;
        };
        let Some(password) = surfaces.take_text() else {
            tracing::warn!(
                "the submitting prompt is no longer the interactive one; not attempting"
            );
            sender.input(finished(failed));
            return;
        };
        let mut result =
            match auth::spawn(self.pam_service.name().to_owned(), user.clone(), password) {
                Ok(result) => result,
                Err(error) => {
                    tracing::error!(%error, "cannot start an authentication thread");
                    sender.input(finished(failed));
                    return;
                }
            };
        let sender = sender.clone();
        relm4::spawn(async move {
            match tokio::time::timeout(auth::TIMEOUT, &mut result).await {
                Ok(verdict) => sender.input(finished(verdict.unwrap_or(failed))),
                Err(_) => {
                    sender.input(AppInput::Lifecycle(Input::AuthTimedOut(attempt)));
                    sender.input(finished(result.await.unwrap_or(failed)));
                }
            }
        });
    }

    fn render(&self) {
        let Some(surfaces) = &self.surfaces else {
            return;
        };
        let view = self.machine.prompt();
        surfaces.render(PromptState {
            available: view.available,
            busy: view.busy,
            message: view.message.as_ref().map(Message::text),
        });
    }

    fn look(&self) -> Look {
        let dark = adw::StyleManager::default().is_dark();
        let lock = &self.config.lock;
        let background = &lock.background;
        let backgrounds = outputs()
            .into_iter()
            .filter_map(|(connector, _)| {
                let texture = self.backgrounds.get(&connector, dark)?.clone();
                Some((connector, texture))
            })
            .collect::<HashMap<_, _>>();
        Look {
            backgrounds,
            color: self.color,
            fit: raster::content_fit(background.fit),
            dim: background::dim(background.dim, background.dim_dark, dark),
            clock: lock.clock.enabled.then(|| {
                (
                    lock.clock.time_format.clone(),
                    lock.clock.date_format.clone(),
                )
            }),
            user: self.display_name.clone().or_else(|| self.user.clone()),
            prompt_output: lock.prompt_output.clone(),
            session: self.session.clone(),
            track: media::track_of(&self.mpris_players, lock.media.enabled),
            chips: self.chip_groups(),
            status: status::shown(&self.status, lock.status.enabled),
        }
    }

    fn chip_groups(&self) -> Vec<glimpse_widgets::ChipGroup> {
        let notifications = &self.config.lock.notifications;
        if !notifications.enabled || self.notif_unavailable {
            return Vec::new();
        }
        let Some(since) = self.lock_started else {
            return Vec::new();
        };
        chips::groups_of(&self.notif_records, since, notifications.privacy)
    }

    fn restage(&self) {
        if let Some(surfaces) = &self.surfaces {
            surfaces.set_look(self.look());
        }
    }

    fn control_track(&self, action: TransportAction) {
        let Some(mpris) = self.mpris.clone() else {
            tracing::warn!("a track action arrived before the mpris service was ready");
            return;
        };
        let Some(player) = self
            .mpris_players
            .players
            .iter()
            .find(|player| player.current)
        else {
            tracing::warn!("a track action arrived with no current player");
            return;
        };
        let action = match action {
            TransportAction::PlayPause => PlayerAction::PlayPause,
            TransportAction::Next => PlayerAction::Next,
            other => {
                tracing::warn!(?other, "the lock stage does not offer this track action");
                return;
            }
        };
        let player = player.id.clone();
        relm4::spawn(async move {
            if let Err(error) = mpris.control(player, action).await {
                tracing::warn!(%error, "a track control failed");
            }
        });
    }

    fn decode(&mut self, sender: &ComponentSender<Self>) {
        let background = &self.config.lock.background;
        let (light, dark) = background::images(&self.config);
        for key in self.backgrounds.due() {
            let image = background::image(light, dark, key.dark).map(ToOwned::to_owned);
            let fit = background.fit;
            let blur_radius = background.blur_radius;
            let sender = sender.clone();
            relm4::spawn_blocking(move || {
                let raster = image
                    .as_deref()
                    .and_then(glimpse_config::resolve_image)
                    .and_then(|path| raster::raster(&path, key.target, fit, blur_radius));
                sender.input(AppInput::Decoded(key, raster));
            });
        }
    }

    fn logind(&self, request: Request) {
        if let Some(logind) = &self.logind
            && let Err(error) = logind.send(request)
        {
            tracing::error!(request = ?error.0, "the logind worker is gone; the request is lost");
        }
    }

    fn reload_styles(&self) {
        let appearance = &self.config.appearance;
        self.styles.load(&Sheets {
            theme: stylesheet(&appearance.theme, LOCK_STYLESHEET),
            theme_dark: stylesheet(&appearance.theme, DARK_STYLESHEET),
            dropin: user_stylesheet(),
            dropin_dark: user_dark_stylesheet(),
        });
        self.styles.set_variant(&appearance.theme_variant);
        self.styles.set_animation_speed(appearance.animation_speed);
    }
}

fn lock_started_on_lock(
    current: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    current.or(Some(now))
}

fn color_scheme(scheme: glimpse_config::ColorScheme) -> adw::ColorScheme {
    match scheme {
        glimpse_config::ColorScheme::Light => adw::ColorScheme::ForceLight,
        glimpse_config::ColorScheme::Dark => adw::ColorScheme::ForceDark,
        glimpse_config::ColorScheme::Auto => adw::ColorScheme::Default,
    }
}

fn hosted(config: &Config) -> Config {
    let mut config = config.clone();
    config.mpris.fetch_art = false;
    config.keyboard.remember = Remember::Global;
    config
}

pub struct StatusServices {
    compositor: Running<Compositor>,
    keyboard: Running<Keyboard>,
    battery: Running<Battery>,
    network: Running<Network>,
    bluetooth: Running<Bluetooth>,
    _weather: WeatherProvider,
    feeds: Vec<JoinHandle<()>>,
}

impl StatusServices {
    fn start(
        config: &Config,
        buses: Buses,
        weather: WeatherProvider,
        sender: &ComponentSender<App>,
    ) -> Self {
        let config = hosted(config);
        let (compositor, compositor_handle) =
            Running::<Compositor>::spawn(&config, buses.clone(), ());
        let (keyboard, layouts) = Running::<Keyboard>::spawn(
            &config,
            buses.clone(),
            KeyboardDependencies {
                compositor: compositor_handle.clone(),
            },
        );
        let (battery, battery_handle) = Running::<Battery>::spawn(&config, buses.clone(), ());
        let (network, network_handle) =
            Running::<Network>::spawn(&config, buses.clone(), NetworkDependencies { agent: false });
        let (bluetooth, bluetooth_handle) =
            Running::<Bluetooth>::spawn(&config, buses, BluetoothDependencies { agent: false });
        let feeds = vec![
            forward_focus(compositor_handle.subscribe(), sender),
            forward(layouts.subscribe(), sender, AppInput::Layouts),
            forward(battery_handle.subscribe(), sender, AppInput::Battery),
            forward_reports(network_handle.subscribe(), sender, AppInput::Network),
            forward(bluetooth_handle.subscribe(), sender, AppInput::Bluetooth),
            forward(weather.handle().subscribe(), sender, AppInput::Weather),
        ];
        Self {
            compositor,
            keyboard,
            battery,
            network,
            bluetooth,
            _weather: weather,
            feeds,
        }
    }

    fn reconfigure(&self, config: &Config) {
        let config = hosted(config);
        self.compositor.reconfigure(&config);
        self.keyboard.reconfigure(&config);
        self.battery.reconfigure(&config);
        self.network.reconfigure(&config);
        self.bluetooth.reconfigure(&config);
    }
}

impl Drop for StatusServices {
    fn drop(&mut self) {
        for feed in &self.feeds {
            feed.abort();
        }
    }
}

fn spawn_services(config: Config, sender: ComponentSender<App>) -> JoinHandle<()> {
    relm4::spawn(async move {
        let buses = Buses::connect().await;
        let (notifications_service, weather) = match buses.session_bus() {
            Ok(connection) => (
                NotificationsProvider::start(connection.clone()),
                WeatherProvider::start(connection.clone()),
            ),
            Err(reason) => (
                NotificationsProvider::unavailable(reason),
                WeatherProvider::unavailable(reason),
            ),
        };
        let notifications = notifications_service.handle();
        let status_services = StatusServices::start(&config, buses.clone(), weather, &sender);
        let (mpris_service, mpris) = Running::<Mpris>::spawn(&hosted(&config), buses, ());
        sender.input(AppInput::ServicesReady {
            mpris_service,
            mpris,
            notifications_service,
            notifications,
            status_services,
        });
    })
}

fn forward<T: Clone + Send + Sync + 'static>(
    mut states: watch::Receiver<T>,
    sender: &ComponentSender<App>,
    wrap: fn(T) -> AppInput,
) -> JoinHandle<()> {
    let sender = sender.clone();
    relm4::spawn(async move {
        sender.input(wrap(states.borrow_and_update().clone()));
        reports(states, |state| sender.input(wrap(state))).await;
    })
}

fn forward_reports<T: Clone + Send + Sync + 'static>(
    states: watch::Receiver<T>,
    sender: &ComponentSender<App>,
    wrap: fn(T) -> AppInput,
) -> JoinHandle<()> {
    let sender = sender.clone();
    relm4::spawn(reports(states, move |state| sender.input(wrap(state))))
}

async fn reports<T: Clone>(mut states: watch::Receiver<T>, send: impl Fn(T)) {
    while states.changed().await.is_ok() {
        send(states.borrow_and_update().clone());
    }
}

fn forward_focus(
    mut states: watch::Receiver<CompositorState>,
    sender: &ComponentSender<App>,
) -> JoinHandle<()> {
    let sender = sender.clone();
    relm4::spawn(async move {
        let mut last = None;
        loop {
            let focused = focused_output(&states.borrow_and_update());
            if focused.is_some() && focused != last {
                sender.input(AppInput::FocusedOutput(focused.clone().flatten()));
                last = focused;
            }
            if states.changed().await.is_err() {
                break;
            }
        }
    })
}

fn focused_output(state: &CompositorState) -> Option<Option<String>> {
    let outputs = state.outputs.as_ref()?;
    Some(
        outputs
            .outputs
            .iter()
            .find(|output| output.focused)
            .map(|output| output.connector.clone()),
    )
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

fn monitors() -> Vec<gdk::Monitor> {
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    let monitors = display.monitors();
    (0..monitors.n_items())
        .filter_map(|index| monitors.item(index)?.downcast::<gdk::Monitor>().ok())
        .collect()
}

fn outputs() -> Vec<(String, raster::Target)> {
    monitors()
        .into_iter()
        .filter_map(|monitor| {
            let connector = monitor.connector()?.to_string();
            let geometry = monitor.geometry();
            if geometry.width() <= 0 || geometry.height() <= 0 {
                return None;
            }
            let scale = match monitor.scale() {
                scale if scale > 0.0 => scale,
                _ => f64::from(monitor.scale_factor()),
            };
            let target = raster::output_target(geometry.width(), geometry.height(), scale);
            Some((connector, target))
        })
        .collect()
}

fn watch_outputs(sender: &ComponentSender<App>) {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    for monitor in monitors() {
        watch_output(&monitor, sender);
    }
    let sender = sender.clone();
    display
        .monitors()
        .connect_items_changed(move |list, position, _, added| {
            for index in position..position + added {
                if let Some(monitor) = list.item(index).and_downcast::<gdk::Monitor>() {
                    watch_output(&monitor, &sender);
                }
            }
            sender.input(AppInput::Outputs);
        });
}

fn watch_output(monitor: &gdk::Monitor, sender: &ComponentSender<App>) {
    let changed = || {
        let sender = sender.clone();
        move |_: &gdk::Monitor| sender.input(AppInput::Outputs)
    };
    monitor.connect_geometry_notify(changed());
    monitor.connect_scale_notify(changed());
    monitor.connect_scale_factor_notify(changed());
    monitor.connect_connector_notify(changed());
}

fn spawn_display_name(sender: ComponentSender<App>) {
    relm4::spawn(async move {
        let fetch = async {
            let uid = glimpse_dbus::login1::current_uid().map_err(|error| error.to_string())?;
            let bus = zbus::Connection::system()
                .await
                .map_err(|error| error.to_string())?;
            let accounts = AccountsProxy::new(&bus)
                .await
                .map_err(|error| error.to_string())?;
            let path = accounts
                .find_user_by_id(i64::from(uid))
                .await
                .map_err(|error| error.to_string())?;
            let account = AccountsUserProxy::builder(&bus)
                .path(path)
                .map_err(|error| error.to_string())?
                .build()
                .await
                .map_err(|error| error.to_string())?;
            let real_name = account.real_name().await.unwrap_or_default();
            let user_name = account.user_name().await.unwrap_or_default();
            Ok::<_, String>(user::display_name(&real_name, &user_name))
        };
        match tokio::time::timeout(ACCOUNTS_TIMEOUT, fetch).await {
            Ok(Ok(name)) => sender.input(AppInput::DisplayName(name)),
            Ok(Err(error)) => tracing::debug!(%error, "no display name from AccountsService"),
            Err(_) => tracing::debug!("AccountsService did not answer in time"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{focused_output, lock_started_on_lock, reports};
    use chrono::TimeZone;
    use glimpse_services::{CompositorOutputs, CompositorState, OutputInfo};
    use std::cell::RefCell;
    use tokio::sync::watch;

    fn output(connector: &str, focused: bool) -> OutputInfo {
        OutputInfo {
            connector: connector.to_owned(),
            label: None,
            built_in: false,
            focused,
            make: None,
            model: None,
            serial: None,
            current_mode: None,
            logical: None,
            enabled: true,
        }
    }

    #[test]
    fn the_focused_output_comes_from_the_compositor_outputs() {
        assert_eq!(
            focused_output(&CompositorState::default()),
            None,
            "no outputs reported yet says nothing, rather than clearing focus"
        );
        let state = CompositorState {
            outputs: Some(CompositorOutputs {
                outputs: vec![output("eDP-1", false), output("DP-2", true)],
            }),
            ..Default::default()
        };
        assert_eq!(focused_output(&state), Some(Some("DP-2".to_owned())));
        let unfocused = CompositorState {
            outputs: Some(CompositorOutputs {
                outputs: vec![output("eDP-1", false)],
            }),
            ..Default::default()
        };
        assert_eq!(focused_output(&unfocused), Some(None));
    }

    #[tokio::test]
    async fn reports_skip_the_placeholder_and_keep_a_value_published_before_subscribing() {
        let (publisher, held) = watch::channel(0);
        let seen = RefCell::new(Vec::new());
        let early = held.clone();
        publisher.send_replace(1);
        let late = held.clone();
        drop(held);
        drop(publisher);
        reports(early, |value| seen.borrow_mut().push(value)).await;
        reports(late, |value| seen.borrow_mut().push(value)).await;
        assert_eq!(
            seen.into_inner(),
            vec![1, 1],
            "a receiver cloned from the never-read original sees a publish from either side"
        );

        let (publisher, held) = watch::channel(0);
        let seen = RefCell::new(Vec::new());
        drop(publisher);
        reports(held, |value| seen.borrow_mut().push(value)).await;
        assert!(
            seen.into_inner().is_empty(),
            "nothing published means nothing reported, so the network slot stays hidden"
        );
    }

    #[test]
    fn an_unrequested_unlock_relock_keeps_the_original_since() {
        let first = chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let later = chrono::Utc.timestamp_opt(1_700_000_060, 0).unwrap();

        let locked = lock_started_on_lock(None, first);
        assert_eq!(locked, Some(first));

        let relocked = lock_started_on_lock(locked, later);
        assert_eq!(
            relocked,
            Some(first),
            "an unrequested-unlock relock never reaches Effect::Unlock, so the original since survives it"
        );
    }

    #[test]
    fn an_authenticated_unlock_then_a_new_lock_resets_since() {
        let first = chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let next = chrono::Utc.timestamp_opt(1_700_000_060, 0).unwrap();

        let locked = lock_started_on_lock(None, first);
        assert_eq!(locked, Some(first));

        let cleared_by_effect_unlock = None;
        let relocked = lock_started_on_lock(cleared_by_effect_unlock, next);
        assert_eq!(
            relocked,
            Some(next),
            "Effect::Unlock clears since; the next lock starts a fresh one"
        );
    }
}
