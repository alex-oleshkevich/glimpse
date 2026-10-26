use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use glimpse_config::{
    Config, Documents, LOCK_STYLESHEET, Reload, Theme, watch_config, watch_theme,
};
use glimpse_dbus::Buses;
use glimpse_dbus::accounts::{AccountsProxy, AccountsUserProxy};
use glimpse_dbus::notifications::{
    NotificationRecord, NotificationsProvider, NotificationsProviderHandle,
    NotificationsProviderState,
};
use glimpse_services::{
    Battery, BatteryState, Compositor, CompositorDependencies, CompositorProvider,
    CompositorProviderHandle, CompositorState, Mpris, MprisHandle, MprisPlayers, PlayerAction,
    Running,
};
use glimpse_widgets::SessionActionState;
use glimpse_widgets::Styles;
use glimpse_widgets::TransportAction;
use glimpse_widgets::actions::ComponentActions;
use glimpse_widgets::color_scheme;
use glimpse_widgets::raster::{self, Raster};
use gtk4::prelude::{
    ApplicationExt, Cast, CastNone, DisplayExt, GskRendererExt, GtkWindowExt, ListModelExt,
    MonitorExt, TextureExt, WidgetExt,
};
use gtk4::{gdk, gio, glib, gsk};
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
const AVATAR_SIDE: i32 = 176;
const ART_SIDE: i32 = 128;

pub struct AppInit {
    pub config: Config,
    pub pam_service: auth::Service,
    pub theme: Theme,
    pub standalone: bool,
    pub cant_verify: bool,
    pub user: Option<String>,
    pub unlocked: Arc<AtomicBool>,
    pub failure: Option<String>,
}

#[allow(clippy::large_enum_variant)]
pub enum AppInput {
    ConfigChanged(Config),
    ConfigVerdict(Result<(), String>),
    Theme(Reload<Theme>),
    Start {
        locked_hint: bool,
        sleep_wait: Duration,
    },
    Lifecycle(Input),
    FocusedOutput(Option<String>),
    Outputs,
    Decoded(Key, Option<Raster>),
    SchemeChanged,
    DisplayName(Option<String>, Option<String>),
    Avatar(u64, String, Option<gdk::Texture>),
    Art(u64, String, glimpse_widgets::Artwork),
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
    Restore(Notice),
}

impl std::fmt::Debug for AppInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::ConfigChanged(_) => "ConfigChanged",
            Self::ConfigVerdict(_) => "ConfigVerdict",
            Self::Theme(_) => "Theme",
            Self::Start { .. } => "Start",
            Self::Lifecycle(_) => "Lifecycle",
            Self::FocusedOutput(_) => "FocusedOutput",
            Self::Outputs => "Outputs",
            Self::Decoded(..) => "Decoded",
            Self::SchemeChanged => "SchemeChanged",
            Self::DisplayName(..) => "DisplayName",
            Self::Avatar(..) => "Avatar",
            Self::Art(..) => "Art",
            Self::SessionAnswers(_) => "SessionAnswers",
            Self::SessionRequested(..) => "SessionRequested",
            Self::SessionResult(..) => "SessionResult",
            Self::BlockInhibitedChanged => "BlockInhibitedChanged",
            Self::ServicesReady { .. } => "ServicesReady",
            Self::Media(_) => "Media",
            Self::Notifications(_) => "Notifications",
            Self::TrackAction(_) => "TrackAction",
            Self::Battery(_) => "Battery",
            Self::Restore(_) => "Restore",
        };
        f.write_str(name)
    }
}

pub struct App {
    actions: ComponentActions,
    _name_owner: Option<gio::OwnerId>,
    config: Config,
    theme: Theme,
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
    blur_renderer: Option<gsk::Renderer>,
    color: gdk::RGBA,
    display_name: Option<String>,
    avatar: AvatarState,
    art: ArtState,
    art_worker: ArtWorker,
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
        let theme_watch = spawn_theme_watch(
            &init.config.appearance.theme,
            init.theme.clone(),
            sender.clone(),
        );
        let actions = ComponentActions::install(
            &relm4::main_application(),
            crate::cli::build::PKG_VERSION,
            init.failure.clone().map_or(Ok(()), Err),
        );
        let name_owner = own_app_name();
        spawn_config_watch(init.config.clone(), init.failure, sender.clone());
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
            actions,
            _name_owner: name_owner,
            config: init.config,
            theme: init.theme,
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
            blur_renderer: None,
            color,
            display_name: None,
            avatar: AvatarState::default(),
            art: ArtState::default(),
            art_worker: ArtWorker::default(),
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

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppInput::ConfigVerdict(verdict) => self.actions.report(verdict),
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
                if self.lock_active() {
                    self.load_art(&sender);
                }
                self.restage();
                if renamed {
                    self.theme_watch.abort();
                    self.theme_watch = spawn_theme_watch(
                        &self.config.appearance.theme,
                        self.theme.clone(),
                        sender.clone(),
                    );
                }
            }
            AppInput::Theme(Reload::Changed(theme)) => {
                self.theme = theme;
                self.reload_styles();
            }
            AppInput::Theme(Reload::Recovered) => tracing::info!("theme recovered"),
            AppInput::Theme(Reload::Rejected(reason)) => {
                tracing::warn!(%reason, "theme rejected");
            }
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
                if self.surfaces.is_none() {
                    self.backgrounds.land(key, None);
                    return;
                }
                if !self.backgrounds.is_wanted(&key) {
                    self.backgrounds.land(key, None);
                    return;
                }
                let failed = raster.is_none().then(|| (key.connector.clone(), key.dark));
                let texture = raster
                    .as_ref()
                    .map(|raster| self.blurred(raster::texture(raster), key.target.width));
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
            AppInput::DisplayName(name, icon) => {
                self.display_name = name;
                if self.lock_active() {
                    self.avatar.icon = icon;
                    self.load_avatar(&sender);
                }
                self.restage();
            }
            AppInput::Avatar(generation, icon, avatar) => {
                match self
                    .avatar
                    .take(generation, &icon, avatar, self.lock_active())
                {
                    AvatarTaken::Applied => self.restage(),
                    AvatarTaken::Reload => self.load_avatar(&sender),
                    AvatarTaken::Stale | AvatarTaken::Dropped => {}
                }
            }
            AppInput::Art(generation, path, loaded) => {
                let current = self.art_wanted().map(ToOwned::to_owned);
                match self.art.take(
                    generation,
                    path,
                    loaded,
                    current.as_deref(),
                    self.lock_active(),
                    Instant::now(),
                ) {
                    ArtTaken::Changed => self.restage(),
                    ArtTaken::Reload => self.load_art(&sender),
                    ArtTaken::Stale | ArtTaken::Dropped | ArtTaken::Same => {}
                }
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
                if self.lock_active() {
                    self.load_art(&sender);
                }
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
            AppInput::Restore(notice) => self.apply(Effect::Notify(notice), &sender),
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
        let was_idle = self.machine.is_idle();
        for effect in self.machine.handle(input) {
            self.apply(effect, sender);
        }
        if self.machine.is_idle() {
            self.surfaces = None;
            self.lock_started = None;
            if !was_idle {
                self.backgrounds.clear();
                self.release_artwork();
                self.release_renderer();
                return_heap();
                for after in HEAP_RETURN_AFTER {
                    glib::timeout_add_local_once(after, return_heap);
                }
            }
        }
        self.render();
    }

    fn apply(&mut self, effect: Effect, sender: &ComponentSender<Self>) {
        match effect {
            Effect::Lock(generation) => {
                tracing::info!(generation, "locking");
                self.session_answers = session::Answers::default();
                self.session = HashMap::new();
                self.lock_started.get_or_insert_with(Utc::now);
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
                self.decode(sender);
                spawn_display_name(sender.clone());
                self.load_avatar(sender);
                self.load_art(sender);
                self.logind(Request::FetchSessionActions);
            }
            Effect::Unlock => {
                tracing::info!("authenticated; unlocking");
                self.lock_started = None;
                self.backgrounds.clear();
                self.release_artwork();
                self.release_renderer();
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
            lasting: view.message.as_ref().is_some_and(Message::lasting),
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
            avatar: self.avatar.shown.clone(),
            prompt_output: lock.prompt_output.clone(),
            session: self.session.clone(),
            track: media::track_of(
                &self.mpris_players,
                lock.media.enabled,
                self.art.shown.as_ref(),
            ),
            notifications: self.chip_groups(),
            status: status::shown(&self.status, lock.status.enabled),
        }
    }

    fn release_artwork(&mut self) {
        self.avatar.release();
        self.art.release();
        self.art_worker.release();
    }

    fn release_renderer(&mut self) {
        if let Some(renderer) = self.blur_renderer.take() {
            renderer.unrealize();
        }
    }

    fn load_avatar(&mut self, sender: &ComponentSender<Self>) {
        let Some(request) = self.avatar.request(self.lock_active()) else {
            return;
        };
        let sender = sender.clone();
        let generation = request.generation;
        let queued = self.art_worker.submit(move || {
            let AvatarRequest { generation, icon } = request;
            let avatar = std::panic::catch_unwind(|| {
                glimpse_widgets::artwork(
                    std::path::Path::new(&icon),
                    AVATAR_SIDE,
                    glimpse_widgets::Formats::Strict,
                )
            })
            .unwrap_or_default();
            sender.input(AppInput::Avatar(generation, icon, avatar));
        });
        if !queued {
            self.avatar.job.finish(generation);
        }
    }

    fn lock_active(&self) -> bool {
        self.surfaces.is_some() && self.machine.is_locking()
    }

    fn art_wanted(&self) -> Option<&str> {
        match self.config.lock.media.enabled {
            true => media::art_path(&self.mpris_players),
            false => None,
        }
    }

    fn load_art(&mut self, sender: &ComponentSender<Self>) {
        let wanted = self.art_wanted().map(ToOwned::to_owned);
        let Some(request) = self
            .art
            .request(wanted.as_deref(), self.lock_active(), Instant::now())
        else {
            return;
        };
        let sender = sender.clone();
        let generation = request.generation;
        let queued = self.art_worker.submit(move || {
            let ArtRequest {
                generation,
                path,
                known,
            } = request;
            let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                glimpse_widgets::cached_artwork(
                    std::path::Path::new(&path),
                    ART_SIDE,
                    glimpse_widgets::Formats::Strict,
                    known.as_ref(),
                )
            }))
            .unwrap_or(glimpse_widgets::Artwork::Rejected(None));
            sender.input(AppInput::Art(generation, path, loaded));
        });
        if !queued {
            self.art.job.finish(generation);
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
        if self.surfaces.is_none() {
            return;
        }
        let background = &self.config.lock.background;
        let (light, dark) = background::images(&self.config);
        for key in self.backgrounds.due() {
            let image = background::image(light, dark, key.dark).map(ToOwned::to_owned);
            let fit = background.fit;
            let sender = sender.clone();
            relm4::spawn_blocking(move || {
                let raster = image
                    .as_deref()
                    .and_then(glimpse_config::resolve_image)
                    .and_then(|path| {
                        raster::raster(&path, key.target, fit)
                            .inspect_err(|error| {
                                tracing::warn!(path = %path.display(), "lock background failed to decode: {error:#}")
                            })
                            .ok()
                    });
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

    fn blurred(&mut self, texture: gdk::Texture, output_width: i32) -> gdk::Texture {
        let radius = self.config.lock.background.blur_radius;
        let Some(sigma) = raster::blur_sigma(radius, texture.width(), output_width) else {
            return texture;
        };
        if self.blur_renderer.is_none() {
            self.blur_renderer = offscreen_renderer();
        }
        self.blur_renderer
            .as_ref()
            .and_then(|renderer| raster::blurred(renderer, &texture, sigma))
            .unwrap_or(texture)
    }

    fn reload_styles(&self) {
        let appearance = &self.config.appearance;
        self.styles.load(&self.theme);
        self.styles.set_variant(&appearance.theme_variant);
        self.styles.set_animation_speed(appearance.animation_speed);
    }
}

const ART_RETRY_AFTER: Duration = Duration::from_secs(30);

const HEAP_RETURN_AFTER: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(35)];

#[derive(Default)]
struct ArtWorker {
    jobs: Option<std::sync::mpsc::Sender<Box<dyn FnOnce() + Send>>>,
}

impl ArtWorker {
    fn submit(&mut self, job: impl FnOnce() + Send + 'static) -> bool {
        if self.jobs.is_none() {
            let (jobs, queue) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();
            let started = std::thread::Builder::new()
                .name("glimpse-lock-art".to_owned())
                .spawn(move || queue.into_iter().for_each(|job| job()));
            if let Err(error) = started {
                tracing::warn!(%error, "cannot start the art decode thread");
                return false;
            }
            self.jobs = Some(jobs);
        }
        let sent = self
            .jobs
            .as_ref()
            .is_some_and(|jobs| jobs.send(Box::new(job)).is_ok());
        if !sent {
            tracing::warn!("the art decode thread is gone");
            self.jobs = None;
        }
        sent
    }

    fn release(&mut self) {
        self.jobs = None;
    }
}

fn return_heap() {
    unsafe { libc::malloc_trim(0) };
}

#[derive(Default)]
struct Job {
    loading: bool,
    generation: u64,
}

impl Job {
    fn begin(&mut self) -> Option<u64> {
        if self.loading {
            return None;
        }
        self.loading = true;
        Some(self.generation)
    }

    fn finish(&mut self, generation: u64) -> bool {
        if generation != self.generation {
            return false;
        }
        self.loading = false;
        true
    }

    fn release(&mut self) {
        self.loading = false;
        self.generation += 1;
    }
}

#[derive(Default)]
struct ArtState {
    shown: Option<(String, gdk::Texture)>,
    seen: Option<(String, glimpse_widgets::Stamp)>,
    failed: Option<(String, Instant)>,
    job: Job,
}

struct ArtRequest {
    generation: u64,
    path: String,
    known: Option<glimpse_widgets::Stamp>,
}

#[derive(Debug, PartialEq, Eq)]
enum ArtTaken {
    Stale,
    Dropped,
    Reload,
    Same,
    Changed,
}

impl ArtState {
    fn request(&mut self, wanted: Option<&str>, locked: bool, now: Instant) -> Option<ArtRequest> {
        let Some(path) = wanted else {
            self.shown = None;
            self.seen = None;
            self.failed = None;
            return None;
        };
        if !locked {
            return None;
        }
        if let Some((failed, at)) = &self.failed
            && failed == path
            && now.saturating_duration_since(*at) < ART_RETRY_AFTER
        {
            return None;
        }
        let generation = self.job.begin()?;
        let known = self
            .seen
            .as_ref()
            .filter(|(seen, _)| seen == path)
            .map(|(_, stamp)| stamp.clone());
        Some(ArtRequest {
            generation,
            path: path.to_owned(),
            known,
        })
    }

    fn take(
        &mut self,
        generation: u64,
        path: String,
        loaded: glimpse_widgets::Artwork,
        wanted: Option<&str>,
        locked: bool,
        now: Instant,
    ) -> ArtTaken {
        if !self.job.finish(generation) {
            return ArtTaken::Stale;
        }
        if !locked {
            return ArtTaken::Dropped;
        }
        if wanted != Some(path.as_str()) {
            return ArtTaken::Reload;
        }
        match loaded {
            glimpse_widgets::Artwork::Unchanged => ArtTaken::Same,
            glimpse_widgets::Artwork::Decoded(texture, stamp) => {
                self.failed = None;
                self.seen = Some((path.clone(), stamp));
                self.shown = Some((path, texture));
                ArtTaken::Changed
            }
            glimpse_widgets::Artwork::Rejected(stamp) => {
                self.failed = match &stamp {
                    Some(_) => None,
                    None => Some((path.clone(), now)),
                };
                self.seen = stamp.map(|stamp| (path, stamp));
                match self.shown.take() {
                    Some(_) => ArtTaken::Changed,
                    None => ArtTaken::Same,
                }
            }
        }
    }

    fn release(&mut self) {
        self.shown = None;
        self.seen = None;
        self.failed = None;
        self.job.release();
    }
}

#[derive(Default)]
struct AvatarState {
    icon: Option<String>,
    shown: Option<gdk::Texture>,
    tried: bool,
    job: Job,
}

struct AvatarRequest {
    generation: u64,
    icon: String,
}

#[derive(Debug, PartialEq, Eq)]
enum AvatarTaken {
    Stale,
    Dropped,
    Reload,
    Applied,
}

impl AvatarState {
    fn request(&mut self, locked: bool) -> Option<AvatarRequest> {
        if !locked || self.tried {
            return None;
        }
        let icon = self.icon.clone().filter(|icon| !icon.is_empty())?;
        let generation = self.job.begin()?;
        self.tried = true;
        Some(AvatarRequest { generation, icon })
    }

    fn take(
        &mut self,
        generation: u64,
        icon: &str,
        avatar: Option<gdk::Texture>,
        locked: bool,
    ) -> AvatarTaken {
        if !self.job.finish(generation) {
            return AvatarTaken::Stale;
        }
        if !locked {
            return AvatarTaken::Dropped;
        }
        if self.icon.as_deref() != Some(icon) {
            self.tried = false;
            return AvatarTaken::Reload;
        }
        self.shown = avatar;
        AvatarTaken::Applied
    }

    fn release(&mut self) {
        self.shown = None;
        self.icon = None;
        self.tried = false;
        self.job.release();
    }
}

fn offscreen_renderer() -> Option<gsk::Renderer> {
    let display = gdk::Display::default()?;
    let renderer = gsk::GLRenderer::new().upcast::<gsk::Renderer>();
    match renderer.realize_for_display(&display) {
        Ok(()) => Some(renderer),
        Err(error) => {
            tracing::warn!("no renderer for the lock background blur; showing it sharp: {error}");
            None
        }
    }
}

fn hosted(config: &Config) -> Config {
    let mut config = config.clone();
    config.mpris.fetch_art = false;
    config
}

pub struct StatusServices {
    compositor: Running<Compositor>,
    battery: Running<Battery>,
    _compositor_provider: CompositorProvider,
    feeds: Vec<JoinHandle<()>>,
}

impl StatusServices {
    fn start(config: &Config, buses: Buses, sender: &ComponentSender<App>) -> Self {
        let config = hosted(config);
        let compositor_provider = match buses.session_bus() {
            Ok(connection) => CompositorProvider::start(connection.clone()),
            Err(reason) => CompositorProvider::unavailable(reason),
        };
        let provider = CompositorProviderHandle(compositor_provider.handle());
        let (compositor, compositor_handle) = Running::<Compositor>::spawn(
            &config,
            buses.clone(),
            CompositorDependencies {
                provider: provider.clone(),
            },
        );
        let (battery, battery_handle) = Running::<Battery>::spawn(&config, buses, <_>::default());
        let feeds = vec![
            forward_focus(compositor_handle.subscribe(), sender),
            forward_restore(provider, sender),
            forward(battery_handle.subscribe(), sender, AppInput::Battery),
        ];
        Self {
            compositor,
            battery,
            _compositor_provider: compositor_provider,
            feeds,
        }
    }

    fn reconfigure(&self, config: &Config) {
        let config = hosted(config);
        self.compositor.reconfigure(&config);
        self.battery.reconfigure(&config);
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
        let notifications_service = match buses.session_bus() {
            Ok(connection) => NotificationsProvider::start(connection.clone()),
            Err(reason) => NotificationsProvider::unavailable(reason),
        };
        let notifications = notifications_service.handle();
        let status_services = StatusServices::start(&config, buses.clone(), &sender);
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

fn forward_restore(
    handle: CompositorProviderHandle,
    sender: &ComponentSender<App>,
) -> JoinHandle<()> {
    let sender = sender.clone();
    relm4::spawn(async move {
        let mut states = handle.subscribe();
        let mut last = None;
        loop {
            let now = states.borrow_and_update().session_lock_restore;
            if let Some(notice) = notify::restore_notice(&mut last, now) {
                sender.input(AppInput::Restore(notice));
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

fn spawn_theme_watch(theme: &str, current: Theme, sender: ComponentSender<App>) -> JoinHandle<()> {
    let themes = watch_theme(theme, LOCK_STYLESHEET, current);
    relm4::spawn(async move {
        let mut themes = Box::pin(themes);
        while let Some(reload) = themes.next().await {
            sender.input(AppInput::Theme(reload));
        }
    })
}

fn spawn_config_watch(current: Config, failure: Option<String>, sender: ComponentSender<App>) {
    relm4::spawn(async move {
        let mut configs = Box::pin(watch_config(Documents::Config, current, failure));
        while let Some(reload) = configs.next().await {
            match reload {
                Reload::Changed(config) => {
                    sender.input(AppInput::ConfigChanged(config));
                    sender.input(AppInput::ConfigVerdict(Ok(())));
                }
                Reload::Recovered => sender.input(AppInput::ConfigVerdict(Ok(()))),
                Reload::Rejected(reason) => sender.input(AppInput::ConfigVerdict(Err(reason))),
            }
        }
    });
}

fn own_app_name() -> Option<gio::OwnerId> {
    let app = relm4::main_application();
    let connection = app.dbus_connection()?;
    let name = app.application_id()?;
    Some(gio::bus_own_name_on_connection(
        &connection,
        &name,
        gio::BusNameOwnerFlags::NONE,
        |_, name| tracing::info!(name, "owns the application name"),
        |_, name| tracing::warn!(name, "cannot own the application name"),
    ))
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
            let uid = glimpse_dbus::login1::current_uid();
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
            let icon = account.icon_file().await.unwrap_or_default();
            Ok::<_, String>((user::display_name(&real_name, &user_name), icon))
        };
        match tokio::time::timeout(ACCOUNTS_TIMEOUT, fetch).await {
            Ok(Ok((name, icon))) => {
                sender.input(AppInput::DisplayName(name, Some(icon)));
            }
            Ok(Err(error)) => tracing::debug!(%error, "no display name from AccountsService"),
            Err(_) => tracing::debug!("AccountsService did not answer in time"),
        }
    });
}

#[cfg(test)]
mod tests;
