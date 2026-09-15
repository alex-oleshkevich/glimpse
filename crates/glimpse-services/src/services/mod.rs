/// What every glimpse process calls itself when it asks a server for something.
pub(crate) const AGENT: &str = concat!("glimpse/", env!("CARGO_PKG_VERSION"));

/// Any error, as the one line a degraded health state or command failure carries.
pub(crate) fn say(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// A request that failed, without the URL in it. `reqwest` puts the whole URL in its `Display`,
/// and a feed address is the user's business rather than the journal's.
pub(crate) fn transport(error: reqwest::Error) -> String {
    match error.is_timeout() {
        true => "the request timed out".to_owned(),
        false => error.without_url().to_string(),
    }
}

mod calendar;
mod compositor;
mod geolocation;
mod heartbeat;
mod keyboard;
mod mpris;
mod night_light;
mod notifications;
mod session;
mod solar;
mod tray;
mod weather;

pub use calendar::{Calendar, CalendarHandle};
pub use compositor::{Compositor, CompositorHandle, CompositorState};
pub use geolocation::{Geolocation, GeolocationHandle};
pub use heartbeat::{Heartbeat, HeartbeatHandle};
pub use keyboard::{Dependencies as KeyboardDependencies, Keyboard, KeyboardHandle};
pub use mpris::{Mpris, MprisHandle};
pub use night_light::{
    Config as NightLightConfig, Dependencies as NightLightDependencies, NightLight,
    NightLightHandle, NightLightState,
};
pub use notifications::{Notifications, NotificationsHandle, NotificationsState};
pub use session::{Dependencies as SessionDependencies, Session, SessionHandle};
pub use solar::{Solar, SolarDependencies, SolarHandle};
pub use tray::{Tray, TrayHandle, TrayItems};
pub use weather::{Config as WeatherConfig, Weather, WeatherDependencies, WeatherHandle};

pub use calendar::{CalendarEvent, CalendarEvents};
pub use compositor::{
    CompositorCapabilities, CompositorOutputs, CompositorPrivacy, CompositorStatus,
    CompositorWindows, CompositorWorkspaces, OutputInfo, WindowInfo, WindowRef, WorkspaceInfo,
    WorkspaceRef,
};
pub use geolocation::GeolocationStatus;
pub use heartbeat::{HeartbeatInterval, HeartbeatTick};
pub use keyboard::{KeyboardLayout, KeyboardLayouts, LayoutRef};
pub use mpris::{MprisPlayers, Playback, PlayerAction, PlayerCapabilities, PlayerStatus, Repeat};
pub use session::SessionStatus;
pub use solar::{SolarPhase, SolarStatus};
