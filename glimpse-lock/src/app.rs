use std::{
    cell::Cell,
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant, UNIX_EPOCH},
};

use css_color::Srgb;
use gio::prelude::SettingsExt;
use glimpse_core::{
    Config, ConfigEvent, FitMode, KeyboardConfig, LockConfig, LockControlButton, ResolvedImageSpec,
    ResolvedLockSpec, ThemeMode, ThemePack, WallpaperConfig,
    dbus::Dbus,
    heic, resolve_lock_spec,
    services::{
        battery::{BatteryHandle, BatteryService, State as BatteryState},
        compositor::CompositorService,
        framework::{Control, ServiceCommand},
        keyboard::{
            Command as KeyboardCommand, KeyboardHandle, KeyboardService, State as KeyboardState,
        },
        network::{NetworkHandle, NetworkService, State as NetworkState},
        session::{Command as SessionCommand, SessionAction, SessionHandle, SessionService},
        theme::EffectiveThemeMode,
    },
    watch_for_config_changes,
};
use gtk4::{
    ContentFit, CssProvider, gdk,
    glib::{
        self,
        object::{Cast, IsA},
    },
    prelude::*,
};
use gtk4_session_lock::Instance;
use notify::event::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmApp,
    SimpleComponent, gtk,
};
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;

use crate::{
    auth::{AuthResult, Authenticator, PamAuthenticator, PreviewAuthenticator, SecretString},
    dbus::{LockApiState, register_lock_api},
    logind::{self, LogindLockEvent},
    runtime::{GTK_APPLICATION_ID, GTK_PREVIEW_APPLICATION_ID, LockRuntime},
    shell_status,
};

const LOCK_CSS_RESOURCE: &str = "/me/aresa/GlimpseLock/lock.css";
const AUTH_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(feature = "dev")]
const DEV_LOCK_CSS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/lock.css");

#[derive(Debug)]
pub enum AppCommand {
    RequestLock,
    RequestUnlock,
    Quit,
    LockFailed,
    Locked,
    Unlocked,
    ReconcileMonitors,
    ApplySharedConfig(Box<Config>),
    ReloadCss,
    ReloadAssets,
    ThemeModeChanged(EffectiveThemeMode),
    SubmitPassword(SecretString),
    AuthFinished { generation: u64, result: AuthResult },
    SleepStarting(zbus::zvariant::OwnedFd),
    RefreshControls,
    ControlStatus(LockControlStatus),
    WeatherState(Option<WeatherDisplay>),
    BatteryState(BatteryState),
    NetworkState(NetworkState),
    KeyboardState(KeyboardState),
    CycleInput,
    PowerAction(LockPowerAction),
    ClockTick,
}

#[derive(Clone, Copy, Debug)]
enum WatchCommand {
    ReloadCss,
    ReloadAssets,
}

pub struct AppInit {
    pub config: LockAppConfig,
    pub authenticator: Arc<dyn Authenticator>,
    pub mode: LockMode,
    pub api_connection: Option<zbus::Connection>,
    pub api_state: LockApiState,
    pub lock_on_start: bool,
}

#[derive(Debug, Clone)]
pub struct LockAppConfig {
    pub lock: LockConfig,
    pub wallpaper: WallpaperConfig,
    pub keyboard: KeyboardConfig,
    pub theme_pack: ThemePack,
    pub theme_mode: ThemeMode,
    pub config_dir: PathBuf,
}

impl LockAppConfig {
    pub fn load() -> Self {
        let shared = Config::load();
        Self::from_shared(shared)
    }

    fn from_shared(shared: Config) -> Self {
        Self {
            theme_pack: shared.theme_pack(),
            theme_mode: shared.theme_mode,
            lock: shared.lock,
            wallpaper: shared.wallpaper,
            keyboard: shared.keyboard,
            config_dir: Config::config_dir(),
        }
    }

    fn resolve(&self, mode: EffectiveThemeMode) -> ResolvedLockSpec {
        resolve_lock_spec(
            &self.lock,
            &self.wallpaper,
            &self.theme_pack,
            mode,
            &self.config_dir,
        )
    }

    fn services_changed(&self, next: &Self) -> bool {
        self.keyboard != next.keyboard
    }

    fn service_config(&self) -> Config {
        Config {
            keyboard: self.keyboard.clone(),
            ..Config::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct UserInfo {
    username: String,
    display_name: String,
    initials: String,
    icon_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockControlStatus {
    /// Wi-Fi/network icon (e.g. "network-wireless-signal-good-symbolic").
    /// None when no network snapshot has arrived yet.
    pub wifi_icon: Option<String>,
    /// None when no battery is present (desktop) or no snapshot has arrived.
    pub battery: Option<BatteryDisplay>,
    /// None when weather is Loading / Unavailable / Unknown.
    pub weather: Option<WeatherDisplay>,
    /// Keyboard layout label. None when no layout is configured or the
    /// keyboard service is unavailable.
    pub input_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatteryDisplay {
    pub icon: String,
    pub percent: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeatherDisplay {
    pub icon: String,
    pub temperature: String,
}

struct LockServices {
    battery: Option<BatteryHandle>,
    network: Option<NetworkHandle>,
    session: Option<SessionHandle>,
    keyboard: KeyboardHandle,
    cancel: CancellationToken,
}

impl Drop for LockServices {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockMode {
    Resident,
    Preview,
}

impl LockMode {
    fn is_preview(self) -> bool {
        self == Self::Preview
    }
}

pub struct LockApp {
    mode: LockMode,
    instance: Option<Instance>,
    runtime: LockRuntime,
    config: LockAppConfig,
    spec: ResolvedLockSpec,
    effective_mode: EffectiveThemeMode,
    authenticator: Arc<dyn Authenticator>,
    windows: Vec<MonitorWindow>,
    preview_window: Option<Controller<LockWindow>>,
    base_css_provider: CssProvider,
    pack_css_provider: CssProvider,
    custom_css_provider: CssProvider,
    _color_scheme_settings: Option<gio::Settings>,
    base_css_watch_cancel: Option<CancellationToken>,
    css_watch_cancel: Option<CancellationToken>,
    pack_css_watch_cancel: Option<CancellationToken>,
    asset_watch_cancel: Option<CancellationToken>,
    user: UserInfo,
    authenticating: bool,
    auth_generation: u64,
    consecutive_failures: u32,
    cooldown_expires: Option<Instant>,
    pending_sleep_release: Option<zbus::zvariant::OwnedFd>,
    control_status: LockControlStatus,
    services: LockServices,
    api_state: LockApiState,
}

#[relm4::component(pub)]
impl SimpleComponent for LockApp {
    type Init = AppInit;
    type Input = AppCommand;
    type Output = ();

    view! {
        gtk::Window {
            set_visible: false,
            set_decorated: false,
            set_deletable: false,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_opacity(0.0);
        let base_css_provider = CssProvider::new();
        let pack_css_provider = CssProvider::new();
        let custom_css_provider = CssProvider::new();
        install_css_provider(&base_css_provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        install_css_provider(&pack_css_provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
        install_css_provider(&custom_css_provider, gtk::STYLE_PROVIDER_PRIORITY_USER + 1);
        load_base_css(&base_css_provider);
        let effective_mode = read_effective_theme_mode(init.config.theme_mode);
        let spec = init.config.resolve(effective_mode);
        let color_scheme_settings = subscribe_color_scheme(sender.clone());
        log_lock_sources(&init.config, &spec, effective_mode);
        load_pack_css(&pack_css_provider, spec.pack_css_path.as_deref());
        load_custom_css(&custom_css_provider, &spec.css_path);

        let (shared_config_tx, mut shared_config_rx) = mpsc::channel(1);
        relm4::spawn(async move {
            watch_for_config_changes(shared_config_tx).await;
        });
        let shared_config_sender = sender.clone();
        relm4::spawn_local(async move {
            while let Some(ConfigEvent::Changed(config)) = shared_config_rx.recv().await {
                shared_config_sender.input(AppCommand::ApplySharedConfig(Box::new(config)));
            }
        });

        if init.mode == LockMode::Resident {
            if let Some(connection) = init.api_connection.clone() {
                let api_sender = sender.input_sender().clone();
                let api_state = init.api_state.clone();
                relm4::spawn(async move {
                    if let Err(error) = register_lock_api(connection, api_sender, api_state).await {
                        tracing::warn!(%error, "failed to register glimpse-lock D-Bus API");
                    }
                });
            }
            let (logind_tx, mut logind_rx) = mpsc::channel(4);
            relm4::spawn(async move {
                const NOISY_FAILURES: u32 = 3;
                let mut retry_delay = Duration::from_secs(1);
                let mut consecutive_failures: u32 = 0;
                loop {
                    match logind::watch_lock_signals(logind_tx.clone()).await {
                        Ok(()) => {
                            retry_delay = Duration::from_secs(1);
                            consecutive_failures = 0;
                        }
                        Err(error) => {
                            consecutive_failures = consecutive_failures.saturating_add(1);
                            if consecutive_failures <= NOISY_FAILURES {
                                tracing::warn!(
                                    error = %format!("{error:#}"),
                                    retry_seconds = retry_delay.as_secs(),
                                    consecutive_failures,
                                    "stopped watching logind lock signals; retrying"
                                );
                            } else {
                                tracing::debug!(
                                    error = %format!("{error:#}"),
                                    retry_seconds = retry_delay.as_secs(),
                                    consecutive_failures,
                                    "stopped watching logind lock signals; retrying"
                                );
                            }
                            sleep(retry_delay).await;
                            retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
                        }
                    }
                }
            });
            let logind_sender = sender.clone();
            relm4::spawn_local(async move {
                while let Some(event) = logind_rx.recv().await {
                    match event {
                        LogindLockEvent::Lock => logind_sender.input(AppCommand::RequestLock),
                        LogindLockEvent::Unlock => logind_sender.input(AppCommand::RequestUnlock),
                        LogindLockEvent::SleepStarting(fd) => {
                            logind_sender.input(AppCommand::SleepStarting(fd));
                        }
                    }
                }
            });
        }

        let services = start_lock_services(&init.config, &sender);
        let mut model = LockApp {
            mode: init.mode,
            instance: None,
            runtime: LockRuntime::default(),
            config: init.config,
            spec,
            effective_mode,
            _color_scheme_settings: Some(color_scheme_settings),
            authenticator: init.authenticator,
            windows: Vec::new(),
            preview_window: None,
            base_css_provider,
            pack_css_provider,
            custom_css_provider,
            base_css_watch_cancel: None,
            css_watch_cancel: None,
            pack_css_watch_cancel: None,
            asset_watch_cancel: None,
            user: current_user_info(),
            authenticating: false,
            auth_generation: 0,
            consecutive_failures: 0,
            cooldown_expires: None,
            pending_sleep_release: None,
            control_status: LockControlStatus::default(),
            services,
            api_state: init.api_state,
        };
        if model.mode == LockMode::Resident {
            connect_monitor_changes(&sender);
        }
        model.watch_base_css(sender.clone());
        model.watch_css(sender.clone());
        model.watch_pack_css(sender.clone());
        model.watch_assets(sender.clone());
        if model.mode == LockMode::Preview {
            model.create_preview_window(sender.clone());
        }
        start_clock_refresh(&sender);
        if model.services.battery.is_none() || model.services.network.is_none() {
            sender.input(AppCommand::RefreshControls);
        }
        if init.lock_on_start && model.mode == LockMode::Resident {
            tracing::info!("logind LockedHint was set at startup; re-acquiring session lock");
            sender.input(AppCommand::RequestLock);
        }
        tracing::info!(
            pam_service = %model.spec.pam_service,
            css_path = %model.spec.css_path.display(),
            mode = ?model.mode,
            "glimpse-lock is running"
        );

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppCommand::RequestLock => self.request_lock(sender),
            AppCommand::RequestUnlock => self.request_unlock(),
            AppCommand::Quit => relm4::main_application().quit(),
            AppCommand::LockFailed => {
                tracing::error!("failed to acquire session lock");
                self.finish_unlock();
            }
            AppCommand::SleepStarting(fd) => {
                if self.instance.is_some() {
                    tracing::info!(
                        "system is preparing to sleep; lock already in place, releasing inhibitor"
                    );
                    drop(fd);
                    return;
                }
                tracing::info!("system is preparing to sleep; locking before suspend");
                self.request_lock(sender);
                if self.instance.is_some() {
                    self.pending_sleep_release = Some(fd);
                } else {
                    tracing::warn!("lock could not be initiated for suspend; releasing inhibitor");
                    drop(fd);
                }
            }
            AppCommand::Locked => {
                tracing::info!("session lock acquired");
                self.runtime.mark_locked();
                self.api_state.set_active(true);
                self.pending_sleep_release.take();
                relm4::spawn(async {
                    if let Err(error) = logind::set_current_session_locked_hint(true).await {
                        tracing::debug!(%error, "failed to set logind LockedHint=true");
                    }
                });
                self.try_unlock();
            }
            AppCommand::Unlocked => {
                if !self.runtime.can_unlock() {
                    tracing::error!(
                        "compositor released the lock without authentication; re-acquiring"
                    );
                    // Force-clear the stale instance so request_lock's `instance.is_some()`
                    // guard doesn't reject. clear_lock_state inside request_lock then handles
                    // the rest of the reset.
                    self.instance = None;
                    self.request_lock(sender);
                    return;
                }
                tracing::info!("session unlocked");
                self.finish_unlock();
            }
            AppCommand::ReconcileMonitors => self.reconcile_monitor_windows(sender),
            AppCommand::ApplySharedConfig(config) => {
                let next_config = LockAppConfig::from_shared(*config);
                if self.config.services_changed(&next_config) {
                    reconfigure_lock_services(&self.services, &next_config);
                }
                self.effective_mode = read_effective_theme_mode(next_config.theme_mode);
                let next = next_config.resolve(self.effective_mode);
                if self.spec == next {
                    self.config = next_config;
                    return;
                }
                tracing::info!("lock shared config changed");
                self.config = next_config;
                self.spec = next;
                log_lock_sources(&self.config, &self.spec, self.effective_mode);
                load_base_css(&self.base_css_provider);
                load_pack_css(&self.pack_css_provider, self.spec.pack_css_path.as_deref());
                load_custom_css(&self.custom_css_provider, &self.spec.css_path);
                self.watch_base_css(sender.clone());
                self.watch_css(sender.clone());
                self.watch_pack_css(sender.clone());
                self.watch_assets(sender.clone());
                self.emit_to_lock_windows(LockWindowInput::Reconfigure(self.spec.clone()));
            }
            AppCommand::ReloadCss => {
                load_base_css(&self.base_css_provider);
                load_pack_css(&self.pack_css_provider, self.spec.pack_css_path.as_deref());
                load_custom_css(&self.custom_css_provider, &self.spec.css_path);
            }
            AppCommand::ThemeModeChanged(mode) => {
                if self.effective_mode == mode {
                    return;
                }
                tracing::info!(?mode, "lock effective theme mode changed");
                self.effective_mode = mode;
                let next = self.config.resolve(mode);
                if self.spec == next {
                    return;
                }
                self.spec = next;
                self.watch_assets(sender.clone());
                self.emit_to_lock_windows(LockWindowInput::Reconfigure(self.spec.clone()));
            }
            AppCommand::ReloadAssets => {
                self.emit_to_lock_windows(LockWindowInput::Reconfigure(self.spec.clone()));
            }
            AppCommand::SubmitPassword(password) => {
                if password.is_empty() {
                    return;
                }
                if self.authenticating {
                    // PAM call is already in flight (the user mashed Enter or
                    // multiple LockWindow instances submitted concurrently).
                    // Silently dropping makes the UI feel unresponsive; keep
                    // the existing "Checking..." status visible so the user
                    // knows something is happening.
                    tracing::debug!("dropping concurrent SubmitPassword while authenticating");
                    return;
                }
                if let Some(remaining) = self.cooldown_remaining() {
                    self.emit_to_lock_windows(LockWindowInput::SetStatus(format!(
                        "Try again in {}s",
                        remaining.as_secs().max(1)
                    )));
                    return;
                }
                self.authenticating = true;
                self.emit_to_lock_windows(LockWindowInput::SetStatus("Checking...".into()));
                let authenticator = self.authenticator.clone();
                let service = self.spec.pam_service.clone();
                let username = self.user.username.clone();
                let result_sender = sender.input_sender().clone();
                let generation = self.auth_generation;
                relm4::spawn_local(async move {
                    let blocking = tokio::task::spawn_blocking(move || {
                        authenticator.authenticate(&service, &username, password)
                    });
                    let auth_result = match timeout(AUTH_TIMEOUT, blocking).await {
                        Err(_elapsed) => {
                            tracing::warn!("PAM authentication timed out after 30s");
                            AuthResult::AccountUnavailable {
                                reason: "Authentication timed out".into(),
                                pam_message: None,
                            }
                        }
                        Ok(join_result) => {
                            let result = join_result
                                .map_err(|error| anyhow::anyhow!("auth worker failed: {error}"))
                                .and_then(|result| result);
                            match result {
                                Ok(result) => result,
                                Err(error) => {
                                    tracing::warn!(%error, "authentication failed");
                                    AuthResult::Failure { pam_message: None }
                                }
                            }
                        }
                    };
                    let _ = result_sender.send(AppCommand::AuthFinished {
                        generation,
                        result: auth_result,
                    });
                });
            }
            AppCommand::AuthFinished { generation, result } => {
                if generation != self.auth_generation {
                    tracing::debug!(
                        stale_generation = generation,
                        current_generation = self.auth_generation,
                        "dropping AuthFinished from cancelled lock cycle"
                    );
                    return;
                }
                self.authenticating = false;
                match result {
                    AuthResult::Success => {
                        tracing::info!("authentication succeeded");
                        self.consecutive_failures = 0;
                        self.cooldown_expires = None;
                        if self.mode.is_preview() {
                            self.emit_to_lock_windows(LockWindowInput::AuthSucceeded);
                            return;
                        }
                        self.runtime.mark_auth_success();
                        self.try_unlock();
                    }
                    AuthResult::Failure { pam_message } => {
                        tracing::warn!("authentication failed");
                        self.runtime.clear_auth_success();
                        self.bump_failure_counter();
                        let status = pam_message.unwrap_or_else(|| self.failure_status_text());
                        self.emit_to_lock_windows(LockWindowInput::SetStatus(status));
                    }
                    AuthResult::AccountUnavailable {
                        reason,
                        pam_message,
                    } => {
                        tracing::warn!(reason = %reason, "PAM account unavailable");
                        self.runtime.clear_auth_success();
                        self.bump_failure_counter();
                        let status = pam_message.unwrap_or(reason);
                        self.emit_to_lock_windows(LockWindowInput::SetStatus(status));
                    }
                }
            }
            AppCommand::RefreshControls => {
                sender.input(AppCommand::ControlStatus(self.control_status.clone()));
            }
            AppCommand::ControlStatus(status) => {
                if self.control_status == status {
                    return;
                }
                self.control_status = status.clone();
                self.emit_to_lock_windows(LockWindowInput::ControlStatus(status));
            }
            AppCommand::WeatherState(state) => {
                let mut status = self.control_status.clone();
                status.weather = state;
                sender.input(AppCommand::ControlStatus(status));
            }
            AppCommand::BatteryState(state) => {
                let mut status = self.control_status.clone();
                status.battery = battery_control_status(&state);
                sender.input(AppCommand::ControlStatus(status));
            }
            AppCommand::NetworkState(state) => {
                let mut status = self.control_status.clone();
                status.wifi_icon = network_control_status(&state);
                sender.input(AppCommand::ControlStatus(status));
            }
            AppCommand::KeyboardState(state) => {
                let mut status = self.control_status.clone();
                status.input_label = keyboard_control_status(&state);
                sender.input(AppCommand::ControlStatus(status));
            }
            AppCommand::CycleInput => {
                cycle_keyboard_layout(&self.services.keyboard);
            }
            AppCommand::PowerAction(action) => {
                if should_run_power_action(self.mode, action) {
                    run_power_action(self.services.session.as_ref(), action);
                }
            }
            AppCommand::ClockTick => {
                self.emit_to_lock_windows(LockWindowInput::ClockTick);
            }
        }
    }
}

impl LockApp {
    fn request_lock(&mut self, sender: ComponentSender<Self>) {
        if self.mode.is_preview() {
            return;
        }
        if self.instance.is_some() {
            tracing::debug!("lock request ignored because lock acquisition is already active");
            return;
        }
        if !gtk4_session_lock::is_supported() {
            tracing::error!("failed to lock session because ext-session-lock is not supported");
            return;
        }
        self.clear_lock_state();
        self.user = current_user_info();

        let instance = Instance::new();
        connect_lock_signals(&instance, &sender);
        if !instance.lock() {
            sender.input(AppCommand::LockFailed);
            return;
        }
        self.instance = Some(instance);
        self.reconcile_monitor_windows(sender);
    }

    fn create_preview_window(&mut self, sender: ComponentSender<Self>) {
        if self.preview_window.is_some() {
            return;
        }
        let window = LockWindow::builder()
            .launch(LockWindowInit {
                spec: self.spec.clone(),
                user: self.user.clone(),
                control_status: self.control_status.clone(),
                sender: sender.input_sender().clone(),
                show_auth: true,
                preview: true,
            })
            .detach();
        window.widget().present();
        self.preview_window = Some(window);
    }

    fn request_unlock(&self) {
        tracing::debug!(
            "logind unlock request ignored because glimpse-lock requires local authentication"
        );
    }

    fn finish_unlock(&mut self) {
        // We are giving up on locking (LockFailed) or have completed a
        // legitimate unlock. Either way the suspend coordinator should let
        // suspend proceed -- drop the inhibitor fd BEFORE clear_lock_state.
        // clear_lock_state deliberately does NOT touch pending_sleep_release
        // so that the re-acquire path (compositor unsolicited release) can
        // hand the same fd through to the new lock cycle without leaking it.
        self.pending_sleep_release.take();
        self.clear_lock_state();
        relm4::spawn(async {
            if let Err(error) = logind::set_current_session_locked_hint(false).await {
                tracing::debug!(%error, "failed to set logind LockedHint=false");
            }
        });
    }

    fn clear_lock_state(&mut self) {
        // NOTE: does NOT touch pending_sleep_release. See finish_unlock for
        // why; the inhibitor fd is owned by the active sleep cycle, not by
        // the lock-instance lifecycle.
        self.windows.clear();
        self.instance = None;
        self.runtime.reset();
        self.authenticating = false;
        self.auth_generation = self.auth_generation.wrapping_add(1);
        self.consecutive_failures = 0;
        self.cooldown_expires = None;
        self.api_state.set_active(false);
    }

    fn cooldown_remaining(&self) -> Option<Duration> {
        let deadline = self.cooldown_expires?;
        deadline.checked_duration_since(Instant::now())
    }
}

/// Cooldown ladder applied between failed unlock attempts. The first
/// failure has no cooldown so an honest typo isn't punished; failure 2
/// gets 1s, then 2/4/8/16 doubling, capped at 30s for failure 7 and
/// beyond. Pure function so the policy can be unit-tested without a
/// `LockApp` test harness.
fn cooldown_seconds_after(failures: u32) -> Option<u64> {
    if failures < 2 {
        return None;
    }
    let exponent = (failures - 2).min(5);
    Some((1u64 << exponent).min(30))
}

impl LockApp {
    fn bump_failure_counter(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if let Some(seconds) = cooldown_seconds_after(self.consecutive_failures) {
            self.cooldown_expires = Some(Instant::now() + Duration::from_secs(seconds));
        }
    }

    fn failure_status_text(&self) -> String {
        match self.cooldown_remaining() {
            Some(remaining) => format!("Try again in {}s", remaining.as_secs().max(1)),
            None => "Authentication failed".to_string(),
        }
    }

    fn emit_to_lock_windows(&self, input: LockWindowInput) {
        for window in &self.windows {
            window.window.emit(input.clone());
        }
        if let Some(window) = &self.preview_window {
            window.emit(input);
        }
    }

    fn reconcile_monitor_windows(&mut self, sender: ComponentSender<Self>) {
        if self.instance.is_none() {
            return;
        }
        let monitors = list_gdk_monitors();
        self.windows.retain(|window| {
            monitors
                .iter()
                .any(|monitor| monitor.as_ptr() == window.monitor.as_ptr())
        });

        for monitor in monitors {
            if self
                .windows
                .iter()
                .any(|window| window.monitor.as_ptr() == monitor.as_ptr())
            {
                continue;
            }
            self.create_monitor_window(monitor, sender.clone());
        }
        self.assign_primary_lock_window();
    }

    fn create_monitor_window(&mut self, monitor: gdk::Monitor, sender: ComponentSender<Self>) {
        let Some(instance) = &self.instance else {
            return;
        };
        let window = LockWindow::builder()
            .launch(LockWindowInit {
                spec: self.spec.clone(),
                user: self.user.clone(),
                control_status: self.control_status.clone(),
                sender: sender.input_sender().clone(),
                show_auth: self.windows.is_empty(),
                preview: false,
            })
            .detach();
        instance.assign_window_to_monitor(window.widget(), &monitor);
        self.windows.push(MonitorWindow { monitor, window });
    }

    fn assign_primary_lock_window(&self) {
        for (index, window) in self.windows.iter().enumerate() {
            window.window.emit(LockWindowInput::SetPrimary(index == 0));
        }
    }

    fn try_unlock(&self) {
        if self.runtime.can_unlock()
            && let Some(instance) = &self.instance
        {
            instance.unlock();
        }
    }

    fn watch_base_css(&mut self, sender: ComponentSender<Self>) {
        if let Some(cancel) = self.base_css_watch_cancel.take() {
            cancel.cancel();
        }
        let Some(path) = dev_lock_css_path() else {
            return;
        };
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let input = sender.input_sender().clone();
        relm4::spawn_local(async move {
            watch_file(path, WatchCommand::ReloadCss, input, task_cancel).await;
        });
        self.base_css_watch_cancel = Some(cancel);
    }

    fn watch_css(&mut self, sender: ComponentSender<Self>) {
        if let Some(cancel) = self.css_watch_cancel.take() {
            cancel.cancel();
        }
        let path = self.spec.css_path.clone();
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let input = sender.input_sender().clone();
        relm4::spawn_local(async move {
            watch_file(path, WatchCommand::ReloadCss, input, task_cancel).await;
        });
        self.css_watch_cancel = Some(cancel);
    }

    fn watch_pack_css(&mut self, sender: ComponentSender<Self>) {
        if let Some(cancel) = self.pack_css_watch_cancel.take() {
            cancel.cancel();
        }
        let Some(path) = self.spec.pack_css_path.clone() else {
            return;
        };
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let input = sender.input_sender().clone();
        relm4::spawn_local(async move {
            watch_file(path, WatchCommand::ReloadCss, input, task_cancel).await;
        });
        self.pack_css_watch_cancel = Some(cancel);
    }

    fn watch_assets(&mut self, sender: ComponentSender<Self>) {
        if let Some(cancel) = self.asset_watch_cancel.take() {
            cancel.cancel();
        }
        let Some(path) = self
            .spec
            .background
            .image
            .as_ref()
            .map(|image| image.path.clone())
        else {
            return;
        };
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let input = sender.input_sender().clone();
        relm4::spawn_local(async move {
            watch_file(path, WatchCommand::ReloadAssets, input, task_cancel).await;
        });
        self.asset_watch_cancel = Some(cancel);
    }
}

struct MonitorWindow {
    monitor: gdk::Monitor,
    window: Controller<LockWindow>,
}

pub fn run(
    config: LockAppConfig,
    args: Vec<String>,
    api_connection: Option<zbus::Connection>,
    api_state: LockApiState,
    lock_on_start: bool,
) -> anyhow::Result<()> {
    let app = RelmApp::new(GTK_APPLICATION_ID);
    app.allow_multiple_instances(true);
    app.visible_on_activate(false)
        .with_args(args)
        .run::<LockApp>(AppInit {
            config,
            authenticator: Arc::new(PamAuthenticator),
            mode: LockMode::Resident,
            api_connection,
            api_state,
            lock_on_start,
        });
    Ok(())
}

pub fn run_preview(config: LockAppConfig, args: Vec<String>) -> anyhow::Result<()> {
    let app = RelmApp::new(GTK_PREVIEW_APPLICATION_ID);
    app.allow_multiple_instances(true);
    app.visible_on_activate(false)
        .with_args(args)
        .run::<LockApp>(AppInit {
            config,
            authenticator: Arc::new(PreviewAuthenticator::default()),
            mode: LockMode::Preview,
            api_connection: None,
            api_state: LockApiState::default(),
            lock_on_start: false,
        });
    Ok(())
}

fn connect_lock_signals(instance: &Instance, sender: &ComponentSender<LockApp>) {
    let failed = sender.input_sender().clone();
    instance.connect_failed(move |_| {
        let _ = failed.send(AppCommand::LockFailed);
    });
    let locked = sender.input_sender().clone();
    instance.connect_locked(move |_| {
        let _ = locked.send(AppCommand::Locked);
    });
    let unlocked = sender.input_sender().clone();
    instance.connect_unlocked(move |_| {
        let _ = unlocked.send(AppCommand::Unlocked);
    });
}

fn connect_monitor_changes(sender: &ComponentSender<LockApp>) {
    let Some(display) = gdk::Display::default() else {
        tracing::warn!("no default GDK display available for lock monitor tracking");
        return;
    };
    let monitor_sender = sender.input_sender().clone();
    display.monitors().connect_items_changed(move |_, _, _, _| {
        let _ = monitor_sender.send(AppCommand::ReconcileMonitors);
    });
}

fn start_lock_services(config: &LockAppConfig, sender: &ComponentSender<LockApp>) -> LockServices {
    let cancel = CancellationToken::new();
    shell_status::spawn(sender.input_sender().clone(), cancel.clone());
    let dbus = match tokio::runtime::Handle::current().block_on(Dbus::connect_async()) {
        Ok(dbus) => dbus,
        Err(error) => {
            tracing::warn!(%error, "failed to connect to D-Bus for lock services");
            return start_lock_services_without_dbus(config, sender, cancel);
        }
    };

    let (battery_service, battery) = BatteryService::new(dbus.system.clone());
    let (network_service, network) = NetworkService::new(dbus.system.clone());
    let (session_service, session) = SessionService::new(dbus.system);
    let (compositor_service, compositor) = CompositorService::new();
    let (keyboard_service, keyboard) = KeyboardService::new(compositor.clone());

    spawn_cancellable_service(cancel.clone(), |task_cancel| {
        battery_service.run(task_cancel)
    });
    spawn_cancellable_service(cancel.clone(), |task_cancel| {
        network_service.run(task_cancel)
    });
    spawn_cancellable_service(cancel.clone(), |task_cancel| {
        session_service.run(task_cancel)
    });
    spawn_cancellable_service(cancel.clone(), |task_cancel| {
        compositor_service.run(task_cancel)
    });
    spawn_cancellable_service(cancel.clone(), |task_cancel| {
        keyboard_service.run(task_cancel)
    });

    start_lock_service_inputs(config, sender, Some(&battery), Some(&network), &keyboard);

    LockServices {
        battery: Some(battery),
        network: Some(network),
        session: Some(session),
        keyboard,
        cancel,
    }
}

fn start_lock_services_without_dbus(
    config: &LockAppConfig,
    sender: &ComponentSender<LockApp>,
    cancel: CancellationToken,
) -> LockServices {
    let (compositor_service, compositor) = CompositorService::new();
    let (keyboard_service, keyboard) = KeyboardService::new(compositor.clone());
    spawn_cancellable_service(cancel.clone(), |task_cancel| {
        compositor_service.run(task_cancel)
    });
    spawn_cancellable_service(cancel.clone(), |task_cancel| {
        keyboard_service.run(task_cancel)
    });
    start_lock_service_inputs(config, sender, None, None, &keyboard);
    LockServices {
        battery: None,
        network: None,
        session: None,
        keyboard,
        cancel,
    }
}

fn spawn_cancellable_service<F, Fut>(cancel: CancellationToken, run: F)
where
    F: FnOnce(CancellationToken) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let task_cancel = cancel.child_token();
    tokio::spawn(async move {
        run(task_cancel).await;
    });
}

fn start_lock_service_inputs(
    config: &LockAppConfig,
    sender: &ComponentSender<LockApp>,
    battery: Option<&BatteryHandle>,
    network: Option<&NetworkHandle>,
    keyboard: &KeyboardHandle,
) {
    start_keyboard_service(keyboard, config);
    if let Some(battery) = battery {
        subscribe_battery_service(battery, sender);
    }
    if let Some(network) = network {
        subscribe_network_service(network, sender);
    }
    subscribe_keyboard_service(keyboard, sender);
}

fn reconfigure_lock_services(services: &LockServices, config: &LockAppConfig) {
    reconfigure_keyboard_service(&services.keyboard, config);
}

fn reconfigure_keyboard_service(keyboard: &KeyboardHandle, config: &LockAppConfig) {
    let keyboard = keyboard.clone();
    let config = config.service_config();
    relm4::spawn(async move {
        if let Err(error) = keyboard
            .send(ServiceCommand::Control(Control::Reconfigure(config)))
            .await
        {
            tracing::warn!(%error, "failed to reconfigure lock keyboard service");
        }
    });
}

fn start_keyboard_service(keyboard: &KeyboardHandle, config: &LockAppConfig) {
    let keyboard = keyboard.clone();
    let config = config.service_config();
    relm4::spawn(async move {
        if let Err(error) = keyboard
            .send(ServiceCommand::Control(Control::Start(config)))
            .await
        {
            tracing::warn!(%error, "failed to start lock keyboard service");
        }
    });
}

fn subscribe_battery_service(battery: &BatteryHandle, sender: &ComponentSender<LockApp>) {
    let mut rx = battery.subscribe();
    let input = sender.input_sender().clone();
    let _ = input.send(AppCommand::BatteryState(rx.borrow().clone()));
    relm4::spawn_local(async move {
        while rx.changed().await.is_ok() {
            let _ = input.send(AppCommand::BatteryState(rx.borrow().clone()));
        }
    });
}

fn subscribe_network_service(network: &NetworkHandle, sender: &ComponentSender<LockApp>) {
    let mut rx = network.subscribe();
    let input = sender.input_sender().clone();
    let _ = input.send(AppCommand::NetworkState(rx.borrow().clone()));
    relm4::spawn_local(async move {
        while rx.changed().await.is_ok() {
            let _ = input.send(AppCommand::NetworkState(rx.borrow().clone()));
        }
    });
}

fn subscribe_keyboard_service(keyboard: &KeyboardHandle, sender: &ComponentSender<LockApp>) {
    let mut rx = keyboard.subscribe();
    let input = sender.input_sender().clone();
    let _ = input.send(AppCommand::KeyboardState(rx.borrow().clone()));
    relm4::spawn_local(async move {
        while rx.changed().await.is_ok() {
            let _ = input.send(AppCommand::KeyboardState(rx.borrow().clone()));
        }
    });
}

fn start_clock_refresh(sender: &ComponentSender<LockApp>) {
    let input = sender.input_sender().clone();
    relm4::spawn_local(async move {
        loop {
            glib::timeout_future(time_to_next_minute()).await;
            if input.send(AppCommand::ClockTick).is_err() {
                break;
            }
        }
    });
}

fn time_to_next_minute() -> Duration {
    use chrono::Timelike;
    let now = chrono::Local::now();
    let secs = u64::from(60 - now.second().min(59));
    let nanos = u64::from(now.nanosecond().min(999_999_999));
    Duration::from_nanos(secs * 1_000_000_000 - nanos).max(Duration::from_millis(100))
}

fn battery_control_status(state: &BatteryState) -> Option<BatteryDisplay> {
    if !state.status.present {
        return None;
    }

    let icon = if state.status.icon_name.is_empty() {
        fallback_battery_icon_name(state.status.percentage).into()
    } else {
        state.status.icon_name.clone()
    };
    let percent = if state.status.on_battery {
        format!("{}%", state.status.percentage)
    } else {
        String::new()
    };
    Some(BatteryDisplay { icon, percent })
}

fn network_control_status(state: &NetworkState) -> Option<String> {
    let snapshot = &state.snapshot;
    if snapshot.status.icon.is_empty() {
        return None;
    }
    Some(snapshot.status.icon.clone())
}

fn keyboard_control_status(state: &KeyboardState) -> Option<String> {
    state
        .current_layout
        .as_ref()
        .filter(|_| state.available)
        .map(|layout| layout.label.clone())
}

fn fallback_battery_icon_name(capacity: u8) -> &'static str {
    match capacity {
        0..=10 => "battery-caution-symbolic",
        11..=30 => "battery-low-symbolic",
        31..=90 => "battery-good-symbolic",
        _ => "battery-full-symbolic",
    }
}

fn list_gdk_monitors() -> Vec<gdk::Monitor> {
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    let monitors = display.monitors();
    (0..monitors.n_items())
        .filter_map(|idx| monitors.item(idx).and_downcast::<gdk::Monitor>())
        .collect()
}

pub struct LockWindow {
    spec: ResolvedLockSpec,
    user: UserInfo,
    status: String,
    controls: LockControlStatus,
    power_menu_open: bool,
    confirm_power_action: Option<LockPowerAction>,
    show_auth: bool,
    caps_lock: bool,
    clock_tick: u64,
    preview: bool,
    background: Controller<BackgroundLayer>,
    sender: relm4::Sender<AppCommand>,
}

pub struct LockWindowInit {
    spec: ResolvedLockSpec,
    user: UserInfo,
    control_status: LockControlStatus,
    sender: relm4::Sender<AppCommand>,
    show_auth: bool,
    preview: bool,
}

#[derive(Clone, Debug)]
pub enum LockWindowInput {
    Reconfigure(ResolvedLockSpec),
    Submit(SecretString),
    SetStatus(String),
    AuthSucceeded,
    AuthFailed,
    PowerAction(LockPowerAction),
    ControlStatus(LockControlStatus),
    CycleInput,
    TogglePowerMenu,
    ClosePowerMenu,
    CancelPowerAction,
    ConfirmPowerAction(LockPowerAction),
    CapsLockChanged(bool),
    SetPrimary(bool),
    ClockTick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockPowerAction {
    Suspend,
    Restart,
    Shutdown,
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for LockWindow {
    type Init = LockWindowInit;
    type Input = LockWindowInput;
    type Output = ();

    view! {
        gtk::Window {
            add_css_class: "lock-window",
            set_decorated: false,
            set_deletable: false,

            gtk::Overlay {
                #[local_ref]
                background_widget -> gtk::Widget {
                },

                add_overlay = &gtk::Box {
                    add_css_class: "lock-content",
                    set_hexpand: true,
                    set_vexpand: true,
                    set_orientation: gtk::Orientation::Vertical,
                    #[watch]
                    set_visible: model.spec.clock.enabled,

                    gtk::Label {
                        add_css_class: "lock-clock",
                        set_halign: gtk::Align::Start,
                        #[watch]
                        set_label: &current_time_label(&model.spec.clock.time_format, model.clock_tick),
                    },

                    gtk::Label {
                        add_css_class: "lock-date",
                        set_halign: gtk::Align::Start,
                        #[watch]
                        set_label: &current_date_label(&model.spec.clock.date_format, model.clock_tick),
                    },
                },

                #[name(auth_panel)]
                add_overlay = &gtk::Box {
                    add_css_class: "lock-auth-panel",
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                    #[watch]
                    set_visible: model.show_auth,

                    gtk::Box {
                        add_css_class: "lock-user-block",
                        set_halign: gtk::Align::Center,
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,

                        #[name(user_avatar)]
                        crate::widgets::Avatar {
                            set_initials: &model.user.initials,
                        },

                        gtk::Label {
                            add_css_class: "lock-user-name",
                            set_halign: gtk::Align::Center,
                            set_label: &model.user.display_name,
                        },
                    },

                    #[name(password_entry)]
                    gtk::PasswordEntry {
                        add_css_class: "lock-password",
                        set_width_chars: 24,
                        set_show_peek_icon: false,
                        connect_activate[sender] => move |entry| {
                            let password = SecretString::new(entry.text().as_str());
                            entry.set_text("");
                            sender.input(LockWindowInput::Submit(password));
                        },
                    },

                    gtk::Label {
                        add_css_class: "lock-caps-indicator",
                        set_label: "Caps Lock",
                        #[watch]
                        set_visible: model.caps_lock,
                    },

                    gtk::Label {
                        add_css_class: "lock-status",
                        #[watch]
                        set_label: &model.status,
                    },
                },

                add_overlay = &gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "lock-power-dismiss",
                    set_halign: gtk::Align::Fill,
                    set_valign: gtk::Align::Fill,
                    set_hexpand: true,
                    set_vexpand: true,
                    set_focusable: false,
                    set_focus_on_click: false,
                    #[watch]
                    set_visible: model.power_menu_open || model.confirm_power_action.is_some(),
                    connect_clicked[sender] => move |_| {
                        sender.input(LockWindowInput::ClosePowerMenu);
                    },
                },

                add_overlay = &gtk::Box {
                    add_css_class: "lock-power-confirm-modal-wrap",
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    set_orientation: gtk::Orientation::Vertical,
                    #[watch]
                    set_visible: model.confirm_power_action.is_some(),

                    gtk::Box {
                        add_css_class: "lock-power-confirm-modal",
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,

                        gtk::Image {
                            add_css_class: "lock-power-confirm-icon",
                            #[watch]
                            set_icon_name: Some(power_confirmation_icon(model.confirm_power_action)),
                        },

                        gtk::Label {
                            add_css_class: "lock-power-confirm-title",
                            set_halign: gtk::Align::Center,
                            set_wrap: true,
                            set_justify: gtk::Justification::Center,
                            #[watch]
                            set_label: &power_confirmation_title(model.confirm_power_action),
                        },

                        gtk::Label {
                            add_css_class: "lock-power-confirm-body",
                            set_halign: gtk::Align::Center,
                            set_wrap: true,
                            set_justify: gtk::Justification::Center,
                            set_label: "Open sessions may lose unsaved work.",
                        },

                        gtk::Box {
                            add_css_class: "lock-power-confirm-actions",
                            set_orientation: gtk::Orientation::Horizontal,
                            set_halign: gtk::Align::Center,
                            set_spacing: 8,

                            gtk::Button {
                                add_css_class: "flat",
                                add_css_class: "lock-power-confirm-button",
                                set_label: "Cancel",
                                connect_clicked[sender] => move |_| {
                                    sender.input(LockWindowInput::CancelPowerAction);
                                },
                            },

                            gtk::Button {
                                add_css_class: "flat",
                                add_css_class: "lock-power-confirm-button",
                                add_css_class: "lock-power-confirm-danger",
                                set_label: "Restart",
                                #[watch]
                                set_visible: model.confirm_power_action == Some(LockPowerAction::Restart),
                                connect_clicked[sender] => move |_| {
                                    sender.input(LockWindowInput::ConfirmPowerAction(LockPowerAction::Restart));
                                },
                            },

                            gtk::Button {
                                add_css_class: "flat",
                                add_css_class: "lock-power-confirm-button",
                                add_css_class: "lock-power-confirm-danger",
                                set_label: "Shut Down",
                                #[watch]
                                set_visible: model.confirm_power_action == Some(LockPowerAction::Shutdown),
                                connect_clicked[sender] => move |_| {
                                    sender.input(LockWindowInput::ConfirmPowerAction(LockPowerAction::Shutdown));
                                },
                            },
                        },
                    },
                },

                #[name(controls)]
                add_overlay = &gtk::Box {
                    add_css_class: "lock-controls",
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::End,
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                    #[watch]
                    set_visible: model.show_auth,
                    #[watch]
                    set_sensitive: model.confirm_power_action.is_none(),

                    gtk::Revealer {
                        add_css_class: "lock-power-revealer",
                        set_halign: gtk::Align::End,
                        set_transition_type: gtk::RevealerTransitionType::SlideUp,
                        set_transition_duration: 140,
                        #[watch]
                        set_reveal_child: model.power_menu_open,
                        #[watch]
                        set_visible: model.power_menu_open && lock_control_enabled(&model.spec, LockControlButton::Power),

                        gtk::Box {
                            add_css_class: "lock-power-menu",
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 2,

                            gtk::Button {
                                add_css_class: "flat",
                                add_css_class: "lock-power-action",
                                connect_clicked[sender] => move |_| {
                                    sender.input(LockWindowInput::PowerAction(LockPowerAction::Suspend));
                                },

                                #[wrap(Some)]
                                set_child = &gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 8,

                                    gtk::Image {
                                        add_css_class: "lock-power-action-icon",
                                        set_icon_name: Some("media-playback-pause-symbolic"),
                                    },

                                    gtk::Label {
                                        add_css_class: "lock-power-action-label",
                                        set_hexpand: true,
                                        set_halign: gtk::Align::Fill,
                                        set_xalign: 0.0,
                                        set_label: "Suspend",
                                    },
                                },
                            },

                            gtk::Button {
                                add_css_class: "flat",
                                add_css_class: "lock-power-action",
                                connect_clicked[sender] => move |_| {
                                    sender.input(LockWindowInput::PowerAction(LockPowerAction::Restart));
                                },

                                #[wrap(Some)]
                                set_child = &gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 8,

                                    gtk::Image {
                                        add_css_class: "lock-power-action-icon",
                                        set_icon_name: Some("view-refresh-symbolic"),
                                    },

                                    gtk::Label {
                                        add_css_class: "lock-power-action-label",
                                        set_hexpand: true,
                                        set_halign: gtk::Align::Fill,
                                        set_xalign: 0.0,
                                        set_label: "Restart",
                                    },
                                },
                            },

                            gtk::Button {
                                add_css_class: "flat",
                                add_css_class: "lock-power-action",
                                connect_clicked[sender] => move |_| {
                                    sender.input(LockWindowInput::PowerAction(LockPowerAction::Shutdown));
                                },

                                #[wrap(Some)]
                                set_child = &gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 8,

                                    gtk::Image {
                                        add_css_class: "lock-power-action-icon",
                                        set_icon_name: Some("system-shutdown-symbolic"),
                                    },

                                    gtk::Label {
                                        add_css_class: "lock-power-action-label",
                                        set_hexpand: true,
                                        set_halign: gtk::Align::Fill,
                                        set_xalign: 0.0,
                                        set_label: "Shutdown",
                                    },
                                },
                            },
                        },
                    },

                    gtk::Box {
                        add_css_class: "lock-control-row",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,

                        #[name(weather_button)]
                        gtk::Button {
                            add_css_class: "flat",
                            add_css_class: "lock-control-button",
                            add_css_class: "lock-control-status-button",
                            set_focusable: false,
                            set_focus_on_click: false,
                            #[watch]
                            set_visible: lock_control_enabled(&model.spec, LockControlButton::Weather)
                                && model.controls.weather.is_some(),

                            #[wrap(Some)]
                            set_child = &gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 6,

                                gtk::Image {
                                    #[watch]
                                    set_icon_name: model.controls.weather.as_ref().map(|w| w.icon.as_str()),
                                },

                                gtk::Label {
                                    add_css_class: "lock-control-value",
                                    #[watch]
                                    set_label: model.controls.weather.as_ref().map(|w| w.temperature.as_str()).unwrap_or(""),
                                },
                            },
                        },

                        #[name(wifi_button)]
                        gtk::Button {
                            add_css_class: "flat",
                            add_css_class: "lock-control-button",
                            add_css_class: "lock-control-status-button",
                            set_focusable: false,
                            set_focus_on_click: false,
                            #[watch]
                            set_icon_name: model.controls.wifi_icon.as_deref().unwrap_or(""),
                            #[watch]
                            set_visible: lock_control_enabled(&model.spec, LockControlButton::Wifi)
                                && model.controls.wifi_icon.is_some(),
                        },

                        #[name(input_button)]
                        gtk::Button {
                            add_css_class: "flat",
                            add_css_class: "lock-control-button",
                            add_css_class: "lock-control-status-button",
                            add_css_class: "lock-input-button",
                            set_focusable: false,
                            set_focus_on_click: false,
                            #[watch]
                            set_label: model.controls.input_label.as_deref().unwrap_or(""),
                            #[watch]
                            set_visible: lock_control_enabled(&model.spec, LockControlButton::Input)
                                && model.controls.input_label.is_some(),
                            connect_clicked[sender] => move |_| {
                                sender.input(LockWindowInput::CycleInput);
                            },
                        },

                        #[name(battery_button)]
                        gtk::Button {
                            add_css_class: "flat",
                            add_css_class: "lock-control-button",
                            add_css_class: "lock-control-status-button",
                            set_focusable: false,
                            set_focus_on_click: false,
                            #[watch]
                            set_visible: lock_control_enabled(&model.spec, LockControlButton::Battery)
                                && model.controls.battery.is_some(),

                            #[wrap(Some)]
                            set_child = &gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 6,

                                gtk::Image {
                                    #[watch]
                                    set_icon_name: model.controls.battery.as_ref().map(|b| b.icon.as_str()),
                                },

                                gtk::Label {
                                    add_css_class: "lock-control-value",
                                    #[watch]
                                    set_label: model.controls.battery.as_ref().map(|b| b.percent.as_str()).unwrap_or(""),
                                },
                            },
                        },

                        #[name(power_button)]
                        gtk::Button {
                            add_css_class: "flat",
                            add_css_class: "lock-control-button",
                            add_css_class: "lock-control-action-button",
                            set_focusable: false,
                            set_focus_on_click: false,
                            set_icon_name: "system-shutdown-symbolic",
                            #[watch]
                            set_visible: lock_control_enabled(&model.spec, LockControlButton::Power),
                            connect_clicked[sender] => move |_| {
                                sender.input(LockWindowInput::TogglePowerMenu);
                            },
                        },
                    },
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let background = BackgroundLayer::builder()
            .launch(init.spec.clone())
            .detach();
        let background_widget_value = background.widget().clone().upcast::<gtk::Widget>();
        let background_widget = &background_widget_value;
        let model = LockWindow {
            spec: init.spec,
            user: init.user,
            status: String::new(),
            controls: init.control_status,
            power_menu_open: false,
            confirm_power_action: None,
            show_auth: init.show_auth,
            caps_lock: false,
            clock_tick: 0,
            preview: init.preview,
            background,
            sender: init.sender,
        };
        let widgets = view_output!();
        connect_lock_window_keys(&root, sender.input_sender().clone());
        connect_caps_lock_indicator(&widgets.password_entry, sender.input_sender().clone());
        widgets
            .user_avatar
            .set_path(model.user.icon_path.as_deref());
        if model.preview {
            root.set_decorated(true);
            root.set_deletable(true);
            root.set_default_size(1280, 720);
            root.set_title(Some("Glimpse Lock Preview"));
            let parent_sender = model.sender.clone();
            root.connect_close_request(move |_| {
                let _ = parent_sender.send(AppCommand::Quit);
                glib::Propagation::Proceed
            });
        }
        if model.show_auth {
            animate_widget_opacity(&widgets.auth_panel, Duration::from_millis(180));
            animate_widget_opacity(&widgets.controls, Duration::from_millis(220));
            gtk4::prelude::GtkWindowExt::set_focus(&root, Some(&widgets.password_entry));
            widgets.password_entry.grab_focus();
            let password_entry = widgets.password_entry.clone();
            glib::idle_add_local_once(move || {
                password_entry.grab_focus();
            });
        }
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            LockWindowInput::Reconfigure(spec) => {
                if !lock_control_enabled(&spec, LockControlButton::Power) {
                    self.power_menu_open = false;
                    self.confirm_power_action = None;
                }
                self.spec = spec.clone();
                self.background.emit(BackgroundInput::Reconfigure(spec));
            }
            LockWindowInput::Submit(password) => {
                let _ = self.sender.send(AppCommand::SubmitPassword(password));
            }
            LockWindowInput::SetStatus(status) => {
                self.status = status;
            }
            LockWindowInput::AuthSucceeded => {
                self.status = "Authentication accepted".into();
                self.power_menu_open = false;
                self.confirm_power_action = None;
            }
            LockWindowInput::AuthFailed => {
                self.status = "Authentication failed".into();
            }
            LockWindowInput::PowerAction(action) => {
                if action.requires_confirmation() && self.confirm_power_action != Some(action) {
                    self.confirm_power_action = Some(action);
                    self.power_menu_open = false;
                    return;
                }
                self.confirm_power_action = None;
                self.power_menu_open = false;
                let _ = self.sender.send(AppCommand::PowerAction(action));
            }
            LockWindowInput::ConfirmPowerAction(action) => {
                if self.confirm_power_action == Some(action) {
                    self.confirm_power_action = None;
                    self.power_menu_open = false;
                    let _ = self.sender.send(AppCommand::PowerAction(action));
                }
            }
            LockWindowInput::ControlStatus(status) => {
                self.controls = status;
            }
            LockWindowInput::CycleInput => {
                self.power_menu_open = false;
                self.confirm_power_action = None;
                let _ = self.sender.send(AppCommand::CycleInput);
            }
            LockWindowInput::TogglePowerMenu => {
                self.power_menu_open = !self.power_menu_open;
                if !self.power_menu_open {
                    self.confirm_power_action = None;
                }
            }
            LockWindowInput::ClosePowerMenu => {
                self.power_menu_open = false;
                self.confirm_power_action = None;
            }
            LockWindowInput::CancelPowerAction => {
                self.confirm_power_action = None;
            }
            LockWindowInput::CapsLockChanged(caps_lock) => {
                if !self.show_auth {
                    return;
                }
                self.caps_lock = caps_lock;
            }
            LockWindowInput::SetPrimary(show_auth) => {
                self.show_auth = show_auth;
                if !show_auth {
                    self.power_menu_open = false;
                    self.confirm_power_action = None;
                    self.caps_lock = false;
                }
            }
            LockWindowInput::ClockTick => {
                self.clock_tick = self.clock_tick.wrapping_add(1);
            }
        }
    }
}

fn lock_control_enabled(spec: &ResolvedLockSpec, button: LockControlButton) -> bool {
    spec.controls.contains(&button)
}

fn animate_widget_opacity<W>(widget: &W, duration: Duration)
where
    W: IsA<gtk::Widget>,
{
    let widget = widget.clone().upcast::<gtk::Widget>();
    if !gtk_animations_enabled() {
        widget.set_opacity(1.0);
        return;
    }
    widget.set_opacity(0.0);
    let started_at = Rc::new(Cell::new(None::<i64>));
    widget.add_tick_callback(move |widget, clock| {
        let start = started_at.get().unwrap_or_else(|| {
            let frame_time = clock.frame_time();
            started_at.set(Some(frame_time));
            frame_time
        });
        let elapsed = (clock.frame_time() - start).max(0) as f64 / 1_000_000.0;
        let duration = duration.as_secs_f64().max(0.001);
        let progress = (elapsed / duration).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - progress).powi(3);
        widget.set_opacity(eased);
        if progress >= 1.0 {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn gtk_animations_enabled() -> bool {
    gtk::Settings::default().is_none_or(|settings| settings.is_gtk_enable_animations())
}

fn power_confirmation_title(action: Option<LockPowerAction>) -> String {
    match action {
        Some(LockPowerAction::Restart) => "Restart this computer?".into(),
        Some(LockPowerAction::Shutdown) => "Shut down this computer?".into(),
        Some(LockPowerAction::Suspend) | None => String::new(),
    }
}

fn power_confirmation_icon(action: Option<LockPowerAction>) -> &'static str {
    match action {
        Some(LockPowerAction::Restart) => "view-refresh-symbolic",
        Some(LockPowerAction::Shutdown) => "system-shutdown-symbolic",
        Some(LockPowerAction::Suspend) | None => "system-shutdown-symbolic",
    }
}

fn connect_lock_window_keys(window: &gtk::Window, sender: relm4::Sender<LockWindowInput>) {
    let controller = gtk::EventControllerKey::new();
    controller.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            let _ = sender.send(LockWindowInput::ClosePowerMenu);
            return glib::Propagation::Stop;
        }

        glib::Propagation::Proceed
    });
    window.add_controller(controller);
}

fn connect_caps_lock_indicator(entry: &gtk::PasswordEntry, sender: relm4::Sender<LockWindowInput>) {
    let controller = gtk::EventControllerKey::new();
    let key_pressed_sender = sender.clone();
    controller.connect_key_pressed(move |_, _, _, state| {
        let _ = key_pressed_sender.send(LockWindowInput::CapsLockChanged(
            state.contains(gdk::ModifierType::LOCK_MASK),
        ));
        glib::Propagation::Proceed
    });
    controller.connect_key_released(move |_, _, _, state| {
        let _ = sender.send(LockWindowInput::CapsLockChanged(
            state.contains(gdk::ModifierType::LOCK_MASK),
        ));
    });
    entry.add_controller(controller);
}

fn run_power_action(session: Option<&SessionHandle>, action: LockPowerAction) {
    let Some(session) = session else {
        tracing::warn!(action = ?action, "lock power action unavailable without session service");
        return;
    };
    let session = session.clone();
    let session_action = action.session_action();
    tracing::info!(action = ?action, "requesting lock power action");
    relm4::spawn(async move {
        if let Err(error) = session
            .send(ServiceCommand::Command(SessionCommand::Run(session_action)))
            .await
        {
            tracing::warn!(action = ?action, %error, "failed to send lock power action");
        }
    });
}

fn should_run_power_action(mode: LockMode, action: LockPowerAction) -> bool {
    if mode.is_preview() {
        tracing::info!(
            action = ?action,
            "lock preview power action ignored; no real session action will be executed"
        );
        return false;
    }

    true
}

impl LockPowerAction {
    fn requires_confirmation(self) -> bool {
        matches!(self, Self::Restart | Self::Shutdown)
    }

    fn session_action(self) -> SessionAction {
        match self {
            Self::Suspend => SessionAction::Suspend,
            Self::Restart => SessionAction::Reboot,
            Self::Shutdown => SessionAction::PowerOff,
        }
    }
}

fn cycle_keyboard_layout(keyboard: &KeyboardHandle) {
    let keyboard = keyboard.clone();
    relm4::spawn(async move {
        if let Err(error) = keyboard
            .send(ServiceCommand::Command(KeyboardCommand::NextLayout))
            .await
        {
            tracing::warn!(%error, "failed to switch keyboard layout");
        }
    });
}

pub struct BackgroundLayer {
    color: gtk::DrawingArea,
    picture: gtk::Picture,
    scrim: gtk::DrawingArea,
    load_state: ImageLoadState,
}

#[derive(Debug)]
pub enum BackgroundInput {
    Reconfigure(ResolvedLockSpec),
    Loaded {
        request_id: u64,
        result: Result<DecodedTexture, String>,
    },
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct TextureTargetSize {
    width: u32,
    height: u32,
}

#[derive(Debug, Default)]
struct ImageLoadState {
    next_request: u64,
    active_request: Option<u64>,
}

impl ImageLoadState {
    fn begin(&mut self) -> u64 {
        self.next_request += 1;
        self.active_request = Some(self.next_request);
        self.next_request
    }

    fn clear(&mut self) {
        self.active_request = None;
    }

    fn is_current(&self, request_id: u64) -> bool {
        self.active_request == Some(request_id)
    }
}

impl BackgroundLayer {
    fn reconfigure(&mut self, spec: ResolvedLockSpec, sender: relm4::Sender<BackgroundInput>) {
        apply_color(&self.color, &spec.background.color);
        apply_dim(&self.scrim, spec.background.dim);
        self.picture
            .set_content_fit(content_fit(spec.background.image.as_ref()));
        if let Some(image) = spec.background.image {
            let target_size = target_texture_size(&self.picture);
            match load_cached_texture_for_image(&image, spec.background.blur_radius, target_size) {
                Ok(Some(decoded)) => {
                    self.load_state.clear();
                    apply_decoded_texture(&self.picture, decoded);
                }
                Ok(None) => {
                    let request_id = self.load_state.begin();
                    spawn_texture_load(
                        request_id,
                        image,
                        spec.background.blur_radius,
                        target_size,
                        sender,
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        path = %image.path.display(),
                        "failed to load cached lock background image before first frame: {error}"
                    );
                    let request_id = self.load_state.begin();
                    spawn_texture_load(
                        request_id,
                        image,
                        spec.background.blur_radius,
                        target_size,
                        sender,
                    );
                }
            }
        } else {
            self.load_state.clear();
            self.picture.set_paintable(None::<&gdk::Paintable>);
        }
    }
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for BackgroundLayer {
    type Init = ResolvedLockSpec;
    type Input = BackgroundInput;
    type Output = ();

    view! {
        gtk::Overlay {
            add_css_class: "lock-background",
            set_hexpand: true,
            set_vexpand: true,

            #[name(color)]
            #[wrap(Some)]
            set_child = &gtk::DrawingArea {
                set_hexpand: true,
                set_vexpand: true,
            },

            #[name(picture)]
            add_overlay = &gtk::Picture {
                set_hexpand: true,
                set_vexpand: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,
                set_can_shrink: true,
            },

            #[name(scrim)]
            add_overlay = &gtk::DrawingArea {
                add_css_class: "lock-scrim",
                set_hexpand: true,
                set_vexpand: true,
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = BackgroundLayer {
            color: gtk::DrawingArea::new(),
            picture: gtk::Picture::new(),
            scrim: gtk::DrawingArea::new(),
            load_state: ImageLoadState::default(),
        };
        let widgets = view_output!();
        let mut model = model;
        model.color = widgets.color.clone();
        model.picture = widgets.picture.clone();
        model.scrim = widgets.scrim.clone();
        model.reconfigure(init, sender.input_sender().clone());
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            BackgroundInput::Reconfigure(spec) => {
                self.reconfigure(spec, sender.input_sender().clone());
            }
            BackgroundInput::Loaded { request_id, result }
                if self.load_state.is_current(request_id) =>
            {
                match result {
                    Ok(decoded) => {
                        apply_decoded_texture(&self.picture, decoded);
                    }
                    Err(error) => {
                        tracing::warn!(
                            "failed to load lock background image; keeping previous image: {error}"
                        );
                    }
                }
            }
            BackgroundInput::Loaded { request_id, .. } => {
                tracing::debug!(request_id, "ignoring stale lock background image load");
            }
        }
    }
}

const GNOME_INTERFACE_SCHEMA: &str = "org.gnome.desktop.interface";
const GNOME_COLOR_SCHEME_KEY: &str = "color-scheme";

fn log_lock_sources(config: &LockAppConfig, spec: &ResolvedLockSpec, mode: EffectiveThemeMode) {
    let (bg_src, bg_path) = if config.lock.background.path.is_some() {
        (
            "config.lock.background",
            config.lock.background.path.as_ref(),
        )
    } else if config.wallpaper.path.is_some() {
        ("config.wallpaper", config.wallpaper.path.as_ref())
    } else if config.theme_pack.lock_bg_for(mode).is_some() {
        (
            "theme.lock-image",
            spec.background.image.as_ref().map(|i| &i.path),
        )
    } else if config.theme_pack.wallpaper_for(mode).is_some() {
        (
            "theme.wallpaper",
            spec.background.image.as_ref().map(|i| &i.path),
        )
    } else {
        ("none", None)
    };
    tracing::info!(
        theme = %config.theme_pack.name,
        mode = ?mode,
        pack_css = spec.pack_css_path.as_ref().map(|p| p.display().to_string()).as_deref().unwrap_or("<none>"),
        override_css = %spec.css_path.display(),
        background_source = bg_src,
        background_path = bg_path.map(|p| p.display().to_string()).as_deref().unwrap_or("<none>"),
        "lock: resolved sources"
    );
}

fn read_effective_theme_mode(configured: ThemeMode) -> EffectiveThemeMode {
    match configured {
        ThemeMode::Light => EffectiveThemeMode::Light,
        ThemeMode::Dark => EffectiveThemeMode::Dark,
        ThemeMode::Auto => {
            let value = gio::Settings::new(GNOME_INTERFACE_SCHEMA).string(GNOME_COLOR_SCHEME_KEY);
            if value == "prefer-dark" {
                EffectiveThemeMode::Dark
            } else {
                EffectiveThemeMode::Light
            }
        }
    }
}

fn subscribe_color_scheme(sender: ComponentSender<LockApp>) -> gio::Settings {
    let settings = gio::Settings::new(GNOME_INTERFACE_SCHEMA);
    settings.connect_changed(Some(GNOME_COLOR_SCHEME_KEY), move |_, _| {
        let config = Config::load();
        let mode = read_effective_theme_mode(config.theme_mode);
        sender.input(AppCommand::ThemeModeChanged(mode));
    });
    settings
}

fn install_css_provider(provider: &CssProvider, priority: u32) {
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, provider, priority);
    }
}

fn load_base_css(provider: &CssProvider) {
    if let Some(path) = dev_lock_css_path()
        && load_css_from_path(provider, &path, "lock dev base CSS")
    {
        return;
    }

    provider.load_from_resource(LOCK_CSS_RESOURCE);
    tracing::info!(source = %LOCK_CSS_RESOURCE, "lock: loaded bundled lock.css");
}

#[cfg(feature = "dev")]
fn dev_lock_css_path() -> Option<PathBuf> {
    Some(PathBuf::from(DEV_LOCK_CSS))
}

#[cfg(not(feature = "dev"))]
fn dev_lock_css_path() -> Option<PathBuf> {
    None
}

fn load_pack_css(provider: &CssProvider, path: Option<&Path>) {
    let Some(path) = path else {
        provider.load_from_string("");
        return;
    };
    if !load_css_from_path(provider, path, "lock pack CSS") {
        provider.load_from_string("");
    }
}

fn load_custom_css(provider: &CssProvider, path: &Path) {
    if !load_css_from_path(provider, path, "lock CSS") {
        provider.load_from_string("");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CssLoadOutcome {
    Applied(String),
    KeptPrevious,
    Missing,
}

fn load_css_from_path(provider: &CssProvider, path: &Path, label: &str) -> bool {
    match css_load_outcome(path, label) {
        CssLoadOutcome::Applied(css) => {
            provider.load_from_string(&css);
            true
        }
        CssLoadOutcome::KeptPrevious => true,
        CssLoadOutcome::Missing => false,
    }
}

fn css_load_outcome(path: &Path, label: &str) -> CssLoadOutcome {
    match fs::read_to_string(path) {
        Ok(css) if css_has_parse_errors(&css) => {
            tracing::warn!(
                path = %path.display(),
                "{label} has parse errors; keeping previous valid CSS"
            );
            CssLoadOutcome::KeptPrevious
        }
        Ok(css) => {
            tracing::info!(path = %path.display(), "loaded {label}");
            CssLoadOutcome::Applied(css)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(path = %path.display(), "{label} not found");
            CssLoadOutcome::Missing
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to read {label}");
            CssLoadOutcome::KeptPrevious
        }
    }
}

fn css_has_parse_errors(css: &str) -> bool {
    let has_error = Rc::new(Cell::new(false));
    let error_marker = has_error.clone();
    let provider = CssProvider::new();
    provider.connect_parsing_error(move |_, section, error| {
        error_marker.set(true);
        tracing::warn!(section = ?section, %error, "lock CSS parse error");
    });
    provider.load_from_string(css);
    has_error.get()
}

async fn watch_file(
    path: PathBuf,
    command: WatchCommand,
    sender: relm4::Sender<AppCommand>,
    cancel: CancellationToken,
) {
    let watch_dirs = watch_dirs_for_file(&path);
    if watch_dirs.is_empty() {
        cancel.cancelled().await;
        return;
    };
    let watched_path = path.canonicalize().unwrap_or(path);
    let mut debouncer = match new_debouncer(
        Duration::from_millis(150),
        None,
        move |res: DebounceEventResult| {
            let Ok(events) = res else {
                return;
            };
            if events.iter().any(|event| {
                file_watch_event_reloads(&event.kind)
                    && event
                        .paths
                        .iter()
                        .any(|path| file_watch_path_matches(path, &watched_path))
            }) {
                let _ = sender.send(match command {
                    WatchCommand::ReloadCss => AppCommand::ReloadCss,
                    WatchCommand::ReloadAssets => AppCommand::ReloadAssets,
                });
            }
        },
    ) {
        Ok(debouncer) => debouncer,
        Err(error) => {
            tracing::warn!(%error, "failed to create lock file watcher");
            return;
        }
    };
    let mut watched_any = false;
    for dir in watch_dirs {
        match debouncer.watch(&dir, notify::RecursiveMode::NonRecursive) {
            Ok(()) => {
                tracing::info!(path = %dir.display(), "watching lock file directory");
                watched_any = true;
            }
            Err(error) => {
                tracing::warn!(path = %dir.display(), %error, "failed to watch lock file directory");
            }
        }
    }
    if !watched_any {
        return;
    }
    cancel.cancelled().await;
}

fn file_watch_event_reloads(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_)
    )
}

fn watch_dirs_for_file(path: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(parent) = path.parent() {
        dirs.push(parent.to_path_buf());
    }
    if let Ok(resolved) = path.canonicalize()
        && let Some(parent) = resolved.parent().map(Path::to_path_buf)
        && !dirs.iter().any(|dir| dir == &parent)
    {
        dirs.push(parent);
    }
    dirs
}

fn file_watch_path_matches(path: &Path, expected: &Path) -> bool {
    path == expected
        || path
            .canonicalize()
            .map(|path| path == expected)
            .unwrap_or(false)
}

/// Best-effort username for the current process. Prefers $USER (cheap, works
/// for the common path), falls back to a `/etc/passwd` lookup by current uid
/// for the case where the env var is unset or empty. Last resort is the
/// literal "user", which feeds PAM and will surface as USER_UNKNOWN -- still
/// better than panicking but the caller should treat it as a degraded state.
fn current_username() -> String {
    if let Ok(name) = std::env::var("USER") {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }
    if let Ok(uid) = glimpse_core::dbus::login1::current_uid()
        && let Some(name) = passwd_username_for_uid(uid)
    {
        return name;
    }
    "user".into()
}

fn passwd_username_for_uid(uid: u32) -> Option<String> {
    fs::read_to_string("/etc/passwd")
        .ok()?
        .lines()
        .find_map(|line| {
            let mut fields = line.split(':');
            let name = fields.next()?;
            let _password = fields.next()?;
            let entry_uid: u32 = fields.next()?.parse().ok()?;
            (entry_uid == uid).then(|| name.to_owned())
        })
}

fn current_user_info() -> UserInfo {
    let username = current_username();
    let (passwd_name, home_dir) = passwd_user_info(&username).unwrap_or_default();
    let display_name = accounts_service_field(&username, "RealName")
        .filter(|name| !name.trim().is_empty())
        .or(passwd_name)
        .unwrap_or_else(|| username.clone());
    let icon_path = accounts_service_field(&username, "Icon")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| user_face_icon(home_dir.as_deref()));
    let initials = user_initials(&display_name, &username);

    UserInfo {
        username,
        display_name,
        initials,
        icon_path,
    }
}

fn accounts_service_field(username: &str, field: &str) -> Option<String> {
    if !is_valid_posix_username(username) {
        tracing::warn!(
            username,
            "rejecting AccountsService lookup for non-POSIX username"
        );
        return None;
    }
    let path = Path::new("/var/lib/AccountsService/users").join(username);
    let prefix = format!("{field}=");
    fs::read_to_string(path).ok()?.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn is_valid_posix_username(username: &str) -> bool {
    if username.is_empty() || username.len() > 32 {
        return false;
    }
    let mut chars = username.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

fn passwd_user_info(username: &str) -> Option<(Option<String>, Option<PathBuf>)> {
    fs::read_to_string("/etc/passwd")
        .ok()?
        .lines()
        .find_map(|line| {
            let mut fields = line.split(':');
            let name = fields.next()?;
            if name != username {
                return None;
            }
            let _password = fields.next()?;
            let _uid = fields.next()?;
            let _gid = fields.next()?;
            let gecos = fields.next().unwrap_or_default();
            let home = fields.next().unwrap_or_default();
            let display_name = gecos
                .split(',')
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let home_dir = (!home.is_empty()).then(|| PathBuf::from(home));
            Some((display_name, home_dir))
        })
}

fn user_face_icon(home_dir: Option<&Path>) -> Option<PathBuf> {
    let home = home_dir
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))?;
    [".face", ".face.icon"]
        .into_iter()
        .map(|file| home.join(file))
        .find(|path| path.is_file())
}

fn user_initials(display_name: &str, username: &str) -> String {
    let initials = display_name
        .split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    if initials.is_empty() {
        username
            .chars()
            .next()
            .map(|ch| ch.to_uppercase().to_string())
            .unwrap_or_else(|| "?".into())
    } else {
        initials
    }
}

fn current_time_label(format: &str, _tick: u64) -> String {
    chrono::Local::now().format(format).to_string()
}

fn current_date_label(format: &str, _tick: u64) -> String {
    chrono::Local::now().format(format).to_string()
}

fn apply_color(area: &gtk::DrawingArea, color: &str) {
    if let Ok(Srgb {
        red,
        green,
        blue,
        alpha,
    }) = color.parse::<Srgb>()
    {
        area.set_draw_func(move |_, cr, _, _| {
            cr.set_source_rgba(red as f64, green as f64, blue as f64, alpha as f64);
            let _ = cr.paint();
        });
        area.queue_draw();
    }
}

fn apply_dim(area: &gtk::DrawingArea, dim: f32) {
    let alpha = dim.clamp(0.0, 1.0) as f64;
    area.set_draw_func(move |_, cr, _, _| {
        cr.set_source_rgba(0.0, 0.0, 0.0, alpha);
        let _ = cr.paint();
    });
    area.queue_draw();
}

fn content_fit(image: Option<&ResolvedImageSpec>) -> ContentFit {
    match image.map(|image| image.fit).unwrap_or(FitMode::Cover) {
        FitMode::Cover => ContentFit::Cover,
        FitMode::Contain => ContentFit::Contain,
        FitMode::Fill => ContentFit::Fill,
    }
}

fn target_texture_size(widget: &gtk::Picture) -> TextureTargetSize {
    let width = widget.width();
    let height = widget.height();
    if width > 0 && height > 0 {
        return TextureTargetSize {
            width: width as u32,
            height: height as u32,
        };
    }
    largest_monitor_size().unwrap_or(TextureTargetSize {
        width: 1920,
        height: 1080,
    })
}

fn largest_monitor_size() -> Option<TextureTargetSize> {
    list_gdk_monitors()
        .into_iter()
        .map(|monitor| {
            let geometry = monitor.geometry();
            TextureTargetSize {
                width: geometry.width().max(1) as u32,
                height: geometry.height().max(1) as u32,
            }
        })
        .max_by_key(|size| size.width.saturating_mul(size.height))
}

fn apply_decoded_texture(picture: &gtk::Picture, decoded: DecodedTexture) {
    let bytes = glib::Bytes::from_owned(decoded.data);
    let texture = gdk::MemoryTexture::new(
        decoded.width,
        decoded.height,
        gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        decoded.stride,
    );
    picture.set_paintable(Some(&texture));
}

fn load_cached_texture_for_image(
    image: &ResolvedImageSpec,
    blur_radius: u32,
    target_size: TextureTargetSize,
) -> anyhow::Result<Option<DecodedTexture>> {
    if !image.path.exists() {
        anyhow::bail!("file not found: {}", image.path.display());
    }
    let cache_key = TextureCacheKey::new(&image.path, image.fit, blur_radius, target_size)?;
    let cached = load_cached_texture(&cache_key)?;
    if let Some(texture) = &cached {
        tracing::info!(
            path = %image.path.display(),
            cache_path = %cache_key.path.display(),
            width = texture.width,
            height = texture.height,
            stride = texture.stride,
            pixel_bytes = texture.data.len(),
            "loaded lock background image from cache before first frame"
        );
    }
    Ok(cached)
}

fn spawn_texture_load(
    request_id: u64,
    image: ResolvedImageSpec,
    blur_radius: u32,
    target_size: TextureTargetSize,
    sender: relm4::Sender<BackgroundInput>,
) {
    relm4::spawn(async move {
        let result =
            tokio::task::spawn_blocking(move || decode_texture(&image, blur_radius, target_size))
                .await
                .map_err(|error| format!("lock background worker failed: {error}"))
                .and_then(|result| result.map_err(|error| error.to_string()));
        let _ = sender.send(BackgroundInput::Loaded { request_id, result });
    });
}

pub struct DecodedTexture {
    width: i32,
    height: i32,
    stride: usize,
    data: Vec<u8>,
}

impl std::fmt::Debug for DecodedTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecodedTexture")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("stride", &self.stride)
            .field("pixel_bytes", &self.data.len())
            .finish()
    }
}

fn decode_texture(
    image: &ResolvedImageSpec,
    blur_radius: u32,
    target_size: TextureTargetSize,
) -> anyhow::Result<DecodedTexture> {
    if !image.path.exists() {
        anyhow::bail!("file not found: {}", image.path.display());
    }
    let cache_key = TextureCacheKey::new(&image.path, image.fit, blur_radius, target_size)?;
    if let Some(cached) = load_cached_texture(&cache_key)? {
        tracing::info!(
            path = %image.path.display(),
            cache_path = %cache_key.path.display(),
            width = cached.width,
            height = cached.height,
            stride = cached.stride,
            pixel_bytes = cached.data.len(),
            "loaded lock background image from cache"
        );
        return Ok(cached);
    }

    let mut rgba = if heic::is_heic_path(&image.path) {
        tracing::info!(path = %image.path.display(), "decoding HEIC lock background image");
        heic::decode(&image.path)?.into_rgba_image()
    } else {
        image::open(&image.path)?.into_rgba8()
    };
    tracing::debug!(
        path = %image.path.display(),
        source_width = rgba.width(),
        source_height = rgba.height(),
        target_width = target_size.width,
        target_height = target_size.height,
        fit = ?image.fit,
        blur_radius,
        "resizing lock background image"
    );
    rgba = resize_rgba_for_fit(rgba, target_size.width, target_size.height, image.fit);
    if blur_radius > 0 {
        rgba = image::imageops::blur(&rgba, blur_radius as f32);
    }
    let (width, height) = rgba.dimensions();
    let decoded = DecodedTexture {
        width: width as i32,
        height: height as i32,
        stride: (width * 4) as usize,
        data: rgba.into_raw(),
    };
    if let Err(error) = write_cached_texture(&cache_key, &decoded) {
        tracing::warn!(
            path = %image.path.display(),
            cache_path = %cache_key.path.display(),
            "failed to update lock background image cache: {error}"
        );
    }
    Ok(decoded)
}

fn resize_rgba_for_fit(
    image: image::RgbaImage,
    width: u32,
    height: u32,
    fit: FitMode,
) -> image::RgbaImage {
    let width = width.max(1);
    let height = height.max(1);
    let image = image::DynamicImage::ImageRgba8(image);
    match fit {
        FitMode::Cover => image
            .resize_to_fill(width, height, image::imageops::FilterType::Lanczos3)
            .into_rgba8(),
        FitMode::Contain => image
            .resize(width, height, image::imageops::FilterType::Lanczos3)
            .into_rgba8(),
        FitMode::Fill => image
            .resize_exact(width, height, image::imageops::FilterType::Lanczos3)
            .into_rgba8(),
    }
}

struct TextureCacheKey {
    path: PathBuf,
}

impl TextureCacheKey {
    fn new(
        source_path: &Path,
        fit: FitMode,
        blur_radius: u32,
        target_size: TextureTargetSize,
    ) -> anyhow::Result<Self> {
        let cache_root = lock_texture_cache_dir();
        let signature = source_signature(source_path)?;
        let mut hasher = DefaultHasher::new();
        "glimpse-lock-rgba-v1".hash(&mut hasher);
        source_path.hash(&mut hasher);
        signature.hash(&mut hasher);
        fit.hash(&mut hasher);
        blur_radius.hash(&mut hasher);
        target_size.hash(&mut hasher);
        let digest = hasher.finish();
        Ok(Self {
            path: cache_root.join(format!("{digest:016x}.rgba")),
        })
    }
}

fn lock_texture_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("glimpse")
        .join("lock")
}

fn source_signature(source_path: &Path) -> anyhow::Result<String> {
    let metadata = fs::metadata(source_path)?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| (duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or((0, 0));
    Ok(format!("{}:{}:{}", metadata.len(), modified.0, modified.1))
}

fn load_cached_texture(cache_key: &TextureCacheKey) -> anyhow::Result<Option<DecodedTexture>> {
    let bytes = match fs::read(&cache_key.path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let Some(header_end) = bytes.iter().position(|byte| *byte == b'\n') else {
        return Ok(None);
    };
    let header = std::str::from_utf8(&bytes[..header_end])?;
    let mut fields = header.split(' ');
    let Some("GLIMPSE_LOCK_RGBA_V1") = fields.next() else {
        return Ok(None);
    };
    let width = parse_cache_i32(fields.next(), "width")?;
    let height = parse_cache_i32(fields.next(), "height")?;
    let stride = parse_cache_usize(fields.next(), "stride")?;
    if fields.next().is_some() || width <= 0 || height <= 0 || stride == 0 {
        return Ok(None);
    }
    let data = bytes[header_end + 1..].to_vec();
    let expected_len = stride.saturating_mul(height as usize);
    if data.len() != expected_len {
        tracing::warn!(
            cache_path = %cache_key.path.display(),
            pixel_bytes = data.len(),
            expected_pixel_bytes = expected_len,
            "ignoring lock background image cache with invalid pixel length"
        );
        return Ok(None);
    }
    Ok(Some(DecodedTexture {
        width,
        height,
        stride,
        data,
    }))
}

fn parse_cache_i32(value: Option<&str>, field: &str) -> anyhow::Result<i32> {
    value
        .ok_or_else(|| anyhow::anyhow!("missing cached image {field}"))?
        .parse()
        .map_err(|error| anyhow::anyhow!("invalid cached image {field}: {error}"))
}

fn parse_cache_usize(value: Option<&str>, field: &str) -> anyhow::Result<usize> {
    value
        .ok_or_else(|| anyhow::anyhow!("missing cached image {field}"))?
        .parse()
        .map_err(|error| anyhow::anyhow!("invalid cached image {field}: {error}"))
}

fn write_cached_texture(
    cache_key: &TextureCacheKey,
    texture: &DecodedTexture,
) -> anyhow::Result<()> {
    if let Some(parent) = cache_key.path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = format!(
        "GLIMPSE_LOCK_RGBA_V1 {} {} {}\n",
        texture.width, texture.height, texture.stride
    )
    .into_bytes();
    bytes.extend_from_slice(&texture.data);
    fs::write(&cache_key.path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use glimpse_core::{
        FitMode,
        services::{
            battery::{
                BatteryState as CoreBatteryState, BatteryStatus, State as CoreBatteryServiceState,
            },
            keyboard::{KeyboardLayout, State as CoreKeyboardState},
            network::{NetworkSnapshot, NetworkStatus, State as CoreNetworkState, WifiAccessPoint},
        },
    };
    use notify::event::{AccessKind, AccessMode, DataChange, ModifyKind, RenameMode};

    use super::{
        CssLoadOutcome, DecodedTexture, ImageLoadState, LockMode, LockPowerAction, TextureCacheKey,
        battery_control_status, cooldown_seconds_after, css_load_outcome, file_watch_event_reloads,
        file_watch_path_matches, keyboard_control_status, load_cached_texture,
        network_control_status, power_confirmation_icon, power_confirmation_title,
        resize_rgba_for_fit, should_run_power_action, watch_dirs_for_file, write_cached_texture,
    };

    #[test]
    fn image_load_state_rejects_stale_requests() {
        let mut state = ImageLoadState::default();

        let first = state.begin();
        let second = state.begin();

        assert!(!state.is_current(first));
        assert!(state.is_current(second));
    }

    #[test]
    fn image_load_state_clear_invalidates_active_request() {
        let mut state = ImageLoadState::default();

        let request = state.begin();
        state.clear();

        assert!(!state.is_current(request));
    }

    #[test]
    fn restart_and_shutdown_power_actions_require_confirmation() {
        assert!(!LockPowerAction::Suspend.requires_confirmation());
        assert!(LockPowerAction::Restart.requires_confirmation());
        assert!(LockPowerAction::Shutdown.requires_confirmation());
    }

    #[test]
    fn preview_mode_never_runs_real_power_actions() {
        assert!(!should_run_power_action(
            LockMode::Preview,
            LockPowerAction::Suspend
        ));
        assert!(!should_run_power_action(
            LockMode::Preview,
            LockPowerAction::Restart
        ));
        assert!(!should_run_power_action(
            LockMode::Preview,
            LockPowerAction::Shutdown
        ));
        assert!(should_run_power_action(
            LockMode::Resident,
            LockPowerAction::Suspend
        ));
    }

    #[test]
    fn power_confirmation_title_is_only_for_destructive_actions() {
        assert_eq!(power_confirmation_title(Some(LockPowerAction::Suspend)), "");
        assert_eq!(
            power_confirmation_title(Some(LockPowerAction::Restart)),
            "Restart this computer?"
        );
        assert_eq!(
            power_confirmation_title(Some(LockPowerAction::Shutdown)),
            "Shut down this computer?"
        );
        assert_eq!(power_confirmation_title(None), "");
    }

    #[test]
    fn power_confirmation_icon_matches_action() {
        assert_eq!(
            power_confirmation_icon(Some(LockPowerAction::Restart)),
            "view-refresh-symbolic"
        );
        assert_eq!(
            power_confirmation_icon(Some(LockPowerAction::Shutdown)),
            "system-shutdown-symbolic"
        );
    }

    #[test]
    fn file_watch_ignores_access_events() {
        assert!(!file_watch_event_reloads(&notify::EventKind::Access(
            AccessKind::Close(AccessMode::Read)
        )));
        assert!(file_watch_event_reloads(&notify::EventKind::Modify(
            notify::event::ModifyKind::Data(DataChange::Content)
        )));
    }

    #[test]
    fn file_watch_reloads_on_editor_save_rename_events() {
        assert!(file_watch_event_reloads(&notify::EventKind::Modify(
            ModifyKind::Name(RenameMode::Both)
        )));
    }

    #[cfg(unix)]
    #[test]
    fn file_watch_handles_symlinked_css_paths() {
        let root = temp_path("watch-symlink");
        let link_dir = root.join("link");
        let target_dir = root.join("target");
        fs::create_dir_all(&link_dir).unwrap();
        fs::create_dir_all(&target_dir).unwrap();
        let target = target_dir.join("lock.css");
        let link = link_dir.join("lock.css");
        fs::write(&target, ".lock-window {}").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let dirs = watch_dirs_for_file(&link);
        assert!(dirs.contains(&link_dir));
        assert!(dirs.contains(&target_dir));
        assert!(file_watch_path_matches(
            &link,
            &target.canonicalize().unwrap()
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn css_load_outcome_marks_missing_files_for_provider_clear() {
        let path = temp_path("missing-lock-css").join("lock.css");

        assert_eq!(css_load_outcome(&path, "lock CSS"), CssLoadOutcome::Missing);
    }

    #[test]
    fn cover_resize_produces_exact_output_size() {
        let image = image::RgbaImage::new(400, 200);
        let resized = resize_rgba_for_fit(image, 100, 100, FitMode::Cover);

        assert_eq!(resized.dimensions(), (100, 100));
    }

    #[test]
    fn contain_resize_preserves_aspect_inside_output_size() {
        let image = image::RgbaImage::new(400, 200);
        let resized = resize_rgba_for_fit(image, 100, 100, FitMode::Contain);

        assert_eq!(resized.dimensions(), (100, 50));
    }

    #[test]
    fn fill_resize_produces_exact_output_size() {
        let image = image::RgbaImage::new(400, 200);
        let resized = resize_rgba_for_fit(image, 100, 100, FitMode::Fill);

        assert_eq!(resized.dimensions(), (100, 100));
    }

    #[test]
    fn battery_control_status_uses_battery_service_state() {
        let mut state = CoreBatteryServiceState::default();
        state.status = BatteryStatus {
            present: true,
            percentage: 57,
            state: CoreBatteryState::Charging,
            icon_name: "battery-good-charging-symbolic".into(),
            on_battery: true,
            ..BatteryStatus::default()
        };

        let display = battery_control_status(&state).expect("present battery yields display");

        assert_eq!(display.icon, "battery-good-charging-symbolic");
        assert_eq!(display.percent, "57%");
    }

    #[test]
    fn battery_control_status_hides_percent_when_on_ac() {
        let mut state = CoreBatteryServiceState::default();
        state.status = BatteryStatus {
            present: true,
            percentage: 89,
            state: CoreBatteryState::Charging,
            icon_name: "battery-good-charging-symbolic".into(),
            on_battery: false,
            ..BatteryStatus::default()
        };

        let display = battery_control_status(&state).expect("present battery yields display");

        assert_eq!(display.icon, "battery-good-charging-symbolic");
        assert_eq!(display.percent, "");
    }

    #[test]
    fn battery_control_status_returns_none_when_no_battery_present() {
        let state = CoreBatteryServiceState::default();
        assert!(battery_control_status(&state).is_none());
    }

    /// Pins the failure-cooldown ladder. This is security-relevant: the
    /// cooldown bounds how fast an attacker (or a kid mashing keys) can
    /// retry. Any change here should be deliberate.
    #[test]
    fn cooldown_ladder_matches_documented_spec() {
        // First failure: no cooldown -- an honest typo shouldn't punish.
        assert_eq!(cooldown_seconds_after(0), None);
        assert_eq!(cooldown_seconds_after(1), None);
        // Doubling ladder 1, 2, 4, 8, 16 for failures 2..6.
        assert_eq!(cooldown_seconds_after(2), Some(1));
        assert_eq!(cooldown_seconds_after(3), Some(2));
        assert_eq!(cooldown_seconds_after(4), Some(4));
        assert_eq!(cooldown_seconds_after(5), Some(8));
        assert_eq!(cooldown_seconds_after(6), Some(16));
        // Cap at 30s from failure 7 onward (32 would be the next double).
        assert_eq!(cooldown_seconds_after(7), Some(30));
        assert_eq!(cooldown_seconds_after(100), Some(30));
        // Extreme inputs still saturate at the cap (no shift overflow).
        assert_eq!(cooldown_seconds_after(u32::MAX), Some(30));
    }

    #[test]
    fn network_control_status_uses_network_service_state() {
        let state = CoreNetworkState {
            snapshot: NetworkSnapshot {
                status: NetworkStatus {
                    enabled: true,
                    wifi_enabled: true,
                    wifi_hw_enabled: true,
                    icon: "network-wireless-signal-good-symbolic".into(),
                    ..NetworkStatus::default()
                },
                wifi_access_points: vec![WifiAccessPoint {
                    ssid: "Studio".into(),
                    connected: true,
                    ..WifiAccessPoint::default()
                }],
                ..NetworkSnapshot::default()
            },
            ..CoreNetworkState::default()
        };

        let icon = network_control_status(&state).expect("snapshot with icon yields Some");

        assert_eq!(icon, "network-wireless-signal-good-symbolic");
    }

    #[test]
    fn network_control_status_returns_none_when_icon_empty() {
        let state = CoreNetworkState::default();
        assert!(network_control_status(&state).is_none());
    }

    #[test]
    fn keyboard_control_status_uses_keyboard_service_state() {
        let state = CoreKeyboardState {
            available: true,
            current_layout: Some(KeyboardLayout {
                index: 1,
                name: "English (US)".into(),
                code: "EN".into(),
                label: "US".into(),
            }),
            current_index: Some(1),
            ..CoreKeyboardState::default()
        };

        let label = keyboard_control_status(&state).expect("layout present yields Some");

        assert_eq!(label, "US");
    }

    #[test]
    fn keyboard_control_status_returns_none_when_unavailable() {
        let state = CoreKeyboardState {
            available: false,
            ..CoreKeyboardState::default()
        };
        assert!(keyboard_control_status(&state).is_none());
    }

    #[test]
    fn decoded_texture_debug_does_not_include_pixel_buffer() {
        let decoded = DecodedTexture {
            width: 1,
            height: 1,
            stride: 4,
            data: vec![1, 2, 3, 4],
        };

        let debug = format!("{decoded:?}");

        assert!(debug.contains("pixel_bytes: 4"));
        assert!(!debug.contains("[1, 2, 3, 4]"));
    }

    #[test]
    fn texture_cache_round_trips_raw_pixels() {
        let cache_dir = temp_path("texture-cache");
        fs::create_dir_all(&cache_dir).unwrap();
        let cache_key = TextureCacheKey {
            path: cache_dir.join("entry.rgba"),
        };
        let decoded = DecodedTexture {
            width: 2,
            height: 1,
            stride: 8,
            data: vec![1, 2, 3, 4, 5, 6, 7, 8],
        };

        write_cached_texture(&cache_key, &decoded).unwrap();
        let cached = load_cached_texture(&cache_key).unwrap().unwrap();

        assert_eq!(cached.width, 2);
        assert_eq!(cached.height, 1);
        assert_eq!(cached.stride, 8);
        assert_eq!(cached.data, vec![1, 2, 3, 4, 5, 6, 7, 8]);

        let _ = fs::remove_dir_all(cache_dir);
    }

    fn temp_path(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("glimpse-lock-{name}-{suffix}"))
    }
}
