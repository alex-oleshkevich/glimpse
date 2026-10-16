mod appearance;
mod applets;
mod bluetooth;
mod brightness;
mod calendar;
mod clipboard;
mod color_format;
mod geolocation;
mod idle;
mod keyboard;
mod lock;
mod mpris;
mod network;
mod night_light;
mod notifications;
mod panels;
mod places;
mod printing;
mod regional;
mod removable;
mod screenshot;
mod wallpaper;
mod weather;

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use appearance::{Appearance, BlurSurface, ColorScheme, DesktopColorScheme, blurs};
pub use applets::{
    Applet, Audio as AudioAppletConfig, BatteryIndicatorStyle, Bluetooth as BluetoothAppletConfig,
    Clipboard as ClipboardAppletConfig, Clock as ClockConfig, Command as CommandAppletConfig,
    Display as DisplayAppletConfig, Exec as ExecAppletConfig, Kind as AppletKind,
    Mpris as MprisAppletConfig, NETWORK_TOOLTIP, NextEvent as NextEventConfig,
    NotificationIndicatorStyle, Notifications as NotificationsAppletConfig, PRINTING_TOOLTIP,
    Pager as PagerConfig, PagerMode, PagerScope, PagerShape, Place as WeatherPlace,
    Places as PlacesAppletConfig, Power as PowerAppletConfig, Printing as PrintingAppletConfig,
    PrintingShow, Privacy as PrivacyAppletConfig, Removable as RemovableAppletConfig,
    Timezone as ClockTimezone, Tray as TrayAppletConfig, WEATHER_TOOLTIP,
    Weather as WeatherAppletConfig, exec_env_name_allowed, is_desktop_id, resolve_applet,
};
pub use bluetooth::Bluetooth;
pub use brightness::Brightness;
pub use calendar::{Calendar, Source as CalendarSource, SourceKind as CalendarSourceKind};
pub use clipboard::Clipboard;
pub use color_format::ColorFormat;
pub use geolocation::Geolocation;
pub use idle::{Idle, Listener as IdleListener, Profile as IdleProfile, Profiles as IdleProfiles};
pub use keyboard::{Keyboard, Remember};
pub use lock::{
    Background as LockBackground, Lock, Privacy as LockPrivacy, Session as LockSession,
    SessionAction as LockSessionAction,
};
pub use mpris::Mpris as MprisConfig;
pub use network::Network as NetworkSettings;
pub use night_light::{CLOCK, MAX_TEMPERATURE, MIN_TEMPERATURE, NightLight, Schedule, parse_clock};
pub use notifications::{NotificationEdge, Notifications};
pub use panels::{Margin, Panel, Position};
pub use places::Places;
pub use printing::Printing;
pub use regional::{HourFormat, Regional, Units as RegionalUnits};
pub use removable::Removable;
pub use screenshot::{Editor, FileTemplate, PaletteColor, Screenshot as ScreenshotConfig, Stroke};
pub use wallpaper::{Backdrop, BackdropOutput, Fit, Transition, Wallpaper, WallpaperOutput};
pub use weather::{Provider as WeatherProvider, Weather as WeatherConfig};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub appearance: Appearance,
    pub regional: Regional,
    pub geolocation: Geolocation,
    pub bluetooth: Bluetooth,
    pub printing: Printing,
    pub network: NetworkSettings,
    pub night_light: NightLight,
    pub brightness: Brightness,
    pub idle: Idle,
    pub keyboard: Keyboard,
    pub calendar: Calendar,
    pub clipboard: Clipboard,
    pub screenshot: ScreenshotConfig,
    pub weather: WeatherConfig,
    pub mpris: MprisConfig,
    pub notifications: Notifications,
    pub wallpaper: Wallpaper,
    pub lock: Lock,
    pub places: Places,
    pub removable: Removable,
    pub panels: Vec<Panel>,
    #[serde(deserialize_with = "applets::deserialize")]
    #[schemars(schema_with = "applets::schema")]
    pub applets: BTreeMap<String, Applet>,
    #[serde(rename = "power", skip_serializing)]
    #[schemars(skip)]
    pub retired_power: toml::Table,
}

impl Config {
    pub fn validate(&self) -> Vec<crate::ConfigError> {
        let mut problems = Vec::new();
        let mut report = |message| problems.push(crate::ConfigError::Schema { message });
        if let Some(problem) = &self.night_light.temperature_problem {
            report(problem.clone());
        }
        if let (Some(start), Some(end)) = (&self.night_light.start_time, &self.night_light.end_time)
            && let (Ok(start), Ok(end)) = (parse_clock(start), parse_clock(end))
            && start == end
        {
            report("night-light.start-time and night-light.end-time must differ".to_owned());
        }
        for (name, profile) in [
            ("ac", &self.idle.profiles.ac),
            ("battery", &self.idle.profiles.battery),
        ] {
            let mut timeouts = std::collections::BTreeSet::new();
            for (index, listener) in profile.listeners.iter().enumerate() {
                if !timeouts.insert(listener.timeout) {
                    report(format!(
                        "idle.profiles.{name}.listeners[{index}].timeout: duplicate timeout {}",
                        listener.timeout
                    ));
                }
                if listener.on_idle.trim().is_empty() {
                    report(format!(
                        "idle.profiles.{name}.listeners[{index}].on-idle: command is empty"
                    ));
                }
            }
        }
        for message in crate::unknown_applets(self) {
            report(message);
        }
        problems
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            regional: Regional::default(),
            geolocation: Geolocation::default(),
            bluetooth: Bluetooth::default(),
            printing: Printing::default(),
            network: NetworkSettings::default(),
            night_light: NightLight::default(),
            brightness: Brightness::default(),
            idle: Idle::default(),
            keyboard: Keyboard::default(),
            calendar: Calendar::default(),
            clipboard: Clipboard::default(),
            screenshot: ScreenshotConfig::default(),
            weather: WeatherConfig::default(),
            mpris: MprisConfig::default(),
            notifications: Notifications::default(),
            wallpaper: Wallpaper::default(),
            lock: Lock::default(),
            places: Places::default(),
            removable: Removable::default(),
            panels: vec![Panel::default()],
            applets: BTreeMap::new(),
            retired_power: toml::Table::new(),
        }
    }
}

/// Every `Kind` actually placed on some panel's `left`/`center`/`right` zone, resolved the same
/// way a panel resolves a zone entry (see `resolve_applet`). The single source of truth for
/// "is anything actually consuming this applet" — a service's demand gating and the panel's own
/// zone resolution must not diverge.
pub fn placed_kinds(config: &Config) -> impl Iterator<Item = AppletKind> + '_ {
    config
        .panels
        .iter()
        .flat_map(|panel| panel.left.iter().chain(&panel.center).chain(&panel.right))
        .filter_map(|name| resolve_applet(name, &config.applets).map(|applet| applet.kind))
}

pub fn placed_applets(config: &Config) -> impl Iterator<Item = (&str, Applet)> + '_ {
    config
        .panels
        .iter()
        .flat_map(|panel| panel.left.iter().chain(&panel.center).chain(&panel.right))
        .filter_map(|name| {
            resolve_applet(name, &config.applets).map(|applet| (name.as_str(), applet))
        })
}

#[cfg(test)]
mod tests;
