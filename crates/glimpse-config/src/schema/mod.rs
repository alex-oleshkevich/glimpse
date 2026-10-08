mod appearance;
mod applets;
mod bluetooth;
mod brightness;
mod calendar;
mod clipboard;
mod color_picker;
mod geolocation;
mod idle;
mod keyboard;
mod lock;
mod monitors;
mod mpris;
mod network;
mod night_light;
mod notifications;
mod panels;
mod places;
mod power;
mod printing;
mod regional;
mod removable;
mod ruler;
mod screenshot;
mod system_monitor;
mod wallpaper;
mod weather;

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use appearance::{Appearance, BlurSurface, ColorScheme, blurs};
pub use applets::{
    Applet, BatteryIndicatorStyle, Bluetooth as BluetoothAppletConfig, Chip as SystemMonitorChip,
    Clipboard as ClipboardAppletConfig, Clock as ClockConfig, Command as CommandAppletConfig,
    Common as AppletCommon, Display as DisplayAppletConfig, Exec as ExecAppletConfig, FirstDay,
    Kdeconnect as KdeconnectAppletConfig, Kind as AppletKind, Mpris as MprisAppletConfig,
    NextEvent as NextEventConfig, NotificationIndicatorStyle,
    Notifications as NotificationsAppletConfig, Pager as PagerConfig, PagerMode, PagerScope,
    PagerShape, Place as WeatherPlace, Places as PlacesAppletConfig, Power as PowerAppletConfig,
    Printing as PrintingAppletConfig, PrintingShow, Privacy as PrivacyAppletConfig,
    Removable as RemovableAppletConfig, Sandbox as AppletSandbox,
    SystemMonitor as SystemMonitorAppletConfig, Timezone as ClockTimezone,
    Tray as TrayAppletConfig, Weather as WeatherAppletConfig, is_desktop_id, renamed_to,
    resolve_applet,
};
pub use bluetooth::Bluetooth;
pub use brightness::Brightness;
pub use calendar::{Calendar, Source as CalendarSource, SourceKind as CalendarSourceKind};
pub use clipboard::Clipboard;
pub use color_picker::{ColorFormat, ColorPicker};
pub use geolocation::Geolocation;
pub use idle::{Idle, Listener as IdleListener, Profile as IdleProfile, Profiles as IdleProfiles};
pub use keyboard::{Keyboard, Remember};
pub use lock::{
    Background as LockBackground, Lock, Privacy as LockPrivacy, Session as LockSession,
    SessionAction as LockSessionAction,
};
pub use monitors::Monitors;
pub use mpris::Mpris as MprisConfig;
pub use network::Network as NetworkSettings;
pub use night_light::{CLOCK, NightLight, Schedule, parse_clock};
pub use notifications::{NotificationEdge, Notifications};
pub use panels::{Margin, Panel, Position};
pub use places::Places;
pub use power::Power;
pub use printing::Printing;
pub use regional::{HourFormat, Regional, Units as RegionalUnits};
pub use removable::Removable;
pub use ruler::Ruler as RulerConfig;
pub use screenshot::{Editor, FileTemplate, PaletteColor, Screenshot as ScreenshotConfig, Stroke};
pub use system_monitor::SystemMonitor;
pub use wallpaper::{Backdrop, BackdropOutput, Fit, Transition, Wallpaper, WallpaperOutput};
pub use weather::{Provider as WeatherProvider, Weather as WeatherConfig};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub appearance: Appearance,
    pub regional: Regional,
    pub monitors: Monitors,
    pub geolocation: Geolocation,
    pub bluetooth: Bluetooth,
    pub printing: Printing,
    pub network: NetworkSettings,
    pub night_light: NightLight,
    pub brightness: Brightness,
    pub idle: Idle,
    pub power: Power,
    pub keyboard: Keyboard,
    pub calendar: Calendar,
    pub clipboard: Clipboard,
    pub color_picker: ColorPicker,
    pub ruler: RulerConfig,
    pub screenshot: ScreenshotConfig,
    pub weather: WeatherConfig,
    pub mpris: MprisConfig,
    pub notifications: Notifications,
    pub wallpaper: Wallpaper,
    pub lock: Lock,
    pub places: Places,
    pub removable: Removable,
    pub system_monitor: SystemMonitor,
    pub panels: Vec<Panel>,
    #[serde(deserialize_with = "applets::deserialize")]
    #[schemars(schema_with = "applets::schema")]
    pub applets: BTreeMap<String, Applet>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            regional: Regional::default(),
            monitors: Monitors::default(),
            geolocation: Geolocation::default(),
            bluetooth: Bluetooth::default(),
            printing: Printing::default(),
            network: NetworkSettings::default(),
            night_light: NightLight::default(),
            brightness: Brightness::default(),
            idle: Idle::default(),
            power: Power::default(),
            keyboard: Keyboard::default(),
            calendar: Calendar::default(),
            clipboard: Clipboard::default(),
            color_picker: ColorPicker::default(),
            ruler: RulerConfig::default(),
            screenshot: ScreenshotConfig::default(),
            weather: WeatherConfig::default(),
            mpris: MprisConfig::default(),
            notifications: Notifications::default(),
            wallpaper: Wallpaper::default(),
            lock: Lock::default(),
            places: Places::default(),
            removable: Removable::default(),
            system_monitor: SystemMonitor::default(),
            panels: vec![Panel::default()],
            applets: BTreeMap::new(),
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
