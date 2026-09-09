/// What every glimpse process calls itself when it asks a server for something.
pub(crate) const AGENT: &str = concat!("glimpse/", env!("CARGO_PKG_VERSION"));

/// Any error, as the one line a `degraded` reason or a `CallError` carries.
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
mod mpris;
mod notifications;
mod solar;
mod weather;

pub use calendar::Calendar;
pub use compositor::Compositor;
pub use geolocation::Geolocation;
pub use heartbeat::Heartbeat;
pub use mpris::Mpris;
pub use notifications::Notifications;
pub use solar::Solar;
pub use weather::Weather;
