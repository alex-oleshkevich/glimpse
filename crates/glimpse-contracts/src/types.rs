use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SolarPhase {
    Day,
    Night,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeoCoordinates {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ServiceState {
    Starting,
    Running,
    Degraded { reason: String },
    Stopped { reason: Option<String> },
}

impl ServiceState {
    /// Whether a topic this service owns should be marked `stale`.
    ///
    /// `stale` means the producer is not running at all, not that it is running badly: a degraded
    /// service keeps publishing what it can, so its values are current and must not be dimmed.
    pub fn is_stale(&self) -> bool {
        !matches!(self, Self::Running | Self::Degraded { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopicReport {
    /// Absent for the broker's own topics, which no service owns and which are never stale.
    pub service: Option<String>,
    pub has_value: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MethodReport {
    pub service: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatInterval {
    pub previous_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub id: u64,
    pub index: Option<u8>,
    pub name: Option<String>,
    pub output: Option<String>,
    pub active: bool,
    pub focused: bool,
    pub urgent: bool,
    pub windows: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u64,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub workspace: Option<u64>,
    pub focused: bool,
    pub floating: bool,
    pub urgent: bool,
    pub order: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputInfo {
    pub connector: String,
    pub label: Option<String>,
    pub built_in: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositorCapabilities {
    pub floating: bool,
    pub workspace_reorder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum WorkspaceRef {
    Id { id: u64 },
    Index { index: u8 },
    Name { name: String },
    Next,
    Prev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum WindowRef {
    Id { id: u64 },
    Pid { pid: i32 },
    Next,
    Prev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum LayoutRef {
    Next,
    Prev,
    Index { index: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardLayout {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub source: String,
    pub summary: String,
    pub detail: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub all_day: bool,
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "snake_case")]
pub enum WatchedPlace {
    Here,
    Coordinates { latitude: f64, longitude: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitSystem {
    Metric,
    Imperial,
}

/// Internally tagged so that `#[serde(other)]` is available: a daemon that learns a new condition
/// must not make an older panel fail to decode the whole payload. Adding a field is already safe,
/// adding a variant is not, and serde offers `other` only on a tagged enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "condition", rename_all = "snake_case")]
pub enum Condition {
    ClearSky,
    MainlyClear,
    PartlyCloudy,
    Overcast,
    Fog,
    Drizzle,
    FreezingDrizzle,
    LightRain,
    Rain,
    HeavyRain,
    FreezingRain,
    LightSnow,
    Snow,
    HeavySnow,
    SnowGrains,
    /// Rain and snow together. WMO 4677 has no code for it and Open-Meteo never sends it; met.no
    /// does, and mapping it onto freezing rain would print the wrong word for wet snow.
    Sleet,
    RainShowers,
    SnowShowers,
    Thunderstorm,
    ThunderstormWithHail,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceWeather {
    pub place: WatchedPlace,
    pub coordinates: GeoCoordinates,
    pub utc_offset_seconds: i32,
    pub current: Option<CurrentWeather>,
    pub hours: Vec<HourForecast>,
    pub days: Vec<DayForecast>,
    #[serde(default)]
    pub alerts: Vec<WeatherAlert>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrentWeather {
    pub observed_at: DateTime<Utc>,
    pub condition: Condition,
    pub is_day: bool,
    pub temperature: f64,
    pub apparent_temperature: Option<f64>,
    pub humidity: Option<u8>,
    pub wind_speed: Option<f64>,
    pub wind_direction: Option<u16>,
    pub precipitation: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HourForecast {
    pub time: DateTime<Utc>,
    pub condition: Condition,
    pub is_day: bool,
    pub temperature: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayForecast {
    pub start: DateTime<Utc>,
    pub condition: Condition,
    pub low: f64,
    pub high: f64,
    pub precipitation_chance: Option<u8>,
    pub sunrise: Option<DateTime<Utc>>,
    pub sunset: Option<DateTime<Utc>>,
}

/// CAP (Common Alerting Protocol) severity, which is what national meteorological services
/// publish, rather than one country's advisory/watch/warning ladder. Internally tagged for the
/// same reason `Condition` is: `#[serde(other)]` is offered only on a tagged enum, so a provider
/// that learns a new severity must not make an older panel fail to decode the whole payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "severity", rename_all = "snake_case")]
pub enum AlertSeverity {
    Minor,
    Moderate,
    Severe,
    Extreme,
    #[serde(other)]
    Unknown,
}

/// Internally tagged for the same reason `Condition` is: `#[serde(other)]` is offered only on a
/// tagged enum, so a player reporting a status this daemon does not know must not make the whole
/// payload fail to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "playback", rename_all = "snake_case")]
pub enum Playback {
    Playing,
    Paused,
    Stopped,
    #[serde(other)]
    Unknown,
}

/// MPRIS `LoopStatus`, which is three states rather than a boolean: repeat-one is a different
/// icon from repeat-all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "repeat", rename_all = "snake_case")]
pub enum Repeat {
    #[default]
    Off,
    Playlist,
    Track,
    #[serde(other)]
    Unknown,
}

/// What a player says it will accept. Shuffle, repeat and volume are absent rather than false when
/// unsupported, which is how the transport hides those controls instead of dimming them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerCapabilities {
    pub play: bool,
    pub pause: bool,
    pub previous: bool,
    pub next: bool,
    pub seek: bool,
    pub control: bool,
    pub raise: bool,
}

/// Every text field is chosen by another application, arrives over the session bus, and is capped
/// by the service before it gets here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerStatus {
    pub id: String,
    pub identity: String,
    pub desktop_entry: Option<String>,
    pub playback: Playback,
    pub current: bool,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// A local path. A remote `mpris:artUrl` is fetched by the service or dropped; it never
    /// reaches a client as a URL to go and load.
    pub art: Option<String>,
    /// Absent or zero for a live stream, which has no end to count towards.
    pub length_us: Option<i64>,
    pub position_us: i64,
    /// When `position_us` was read. `Position` emits no change signal, so a client advances it
    /// locally from here rather than the daemon republishing once a second.
    pub position_at: DateTime<Utc>,
    pub rate: f64,
    pub volume: Option<f64>,
    pub repeat: Option<Repeat>,
    pub shuffle: Option<bool>,
    pub can: PlayerCapabilities,
}

/// What a client asks a player to do. Tagged so a client built against an older daemon still
/// serializes something the newer one can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PlayerAction {
    Play,
    Pause,
    PlayPause,
    Stop,
    Previous,
    Next,
    Raise,
}

/// Urgency as the freedesktop specification defines it. Internally tagged for the same reason
/// `Condition` is: a sender may send a byte outside the three the specification names, and that
/// must not make the whole payload fail to decode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "urgency", rename_all = "snake_case")]
pub enum NotificationUrgency {
    Low,
    #[default]
    Normal,
    Critical,
    #[serde(other)]
    Unknown,
}

/// One action a sender offered. `key` is what goes back to it over the bus; `label` is what the
/// reader sees, and is third-party text like every other string here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationAction {
    pub key: String,
    pub label: String,
}

pub const DEFAULT_ACTION: &str = "default";

/// One notification as the store holds it. Every text field is chosen by another application,
/// arrives over the session bus, and is capped and sanitised by the service before it gets here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotificationRecord {
    /// The id the specification handed the sender, and what every command names it by.
    pub id: u32,
    /// The sender's own identity: its `desktop-entry` hint where it gave one, its bus name
    /// otherwise. Grouping keys on this rather than on `app_name`, which a sender chooses freely
    /// and could therefore borrow from somebody else.
    pub app_id: String,
    pub app_name: String,
    /// The process that sent this notification, where the bus could name one, so a client can ask
    /// the compositor to raise that process's window. Raising belongs to the specification's
    /// `default` action, which means the reader activated the notification itself; a button the
    /// sender named is a command, and there only the activation token decides. A portal-relayed
    /// sender resolves to the portal rather than to the application, and then there is nothing to
    /// raise.
    pub app_pid: Option<i32>,
    pub summary: String,
    /// Sanitised Pango markup, safe to hand to `set_markup`. The freedesktop specification makes
    /// markup a server capability rather than a per-notification flag, so there is no "is this
    /// markup" boolean to carry: every body has been through `glimpse_utils::markup::sanitize_body`
    /// by the time it is here. A client that cannot render markup strips it.
    pub body: Option<String>,
    /// A themed icon name, never a path a client should go and load.
    pub icon: Option<String>,
    /// An absolute local path supplied by the sender through an image hint.
    pub image: Option<String>,
    pub urgency: NotificationUrgency,
    pub actions: Vec<NotificationAction>,
    /// The `value` hint, as a fraction. Present only when the sender sent one.
    pub progress: Option<f64>,
    pub created: DateTime<Utc>,
    pub unread: bool,
    /// The sender asked to stay until it is acted on. `resident` and a zero timeout are the two
    /// ways it can say so, and the store treats them alike.
    pub resident: bool,
}

/// Do not disturb. `until` is when it lapses on its own; `None` means it stands until the reader
/// turns it off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoNotDisturb {
    pub enabled: bool,
    pub until: Option<DateTime<Utc>>,
}

/// The only prose in this payload that glimpse did not format itself. Every field carrying text is
/// third-party, arrives over the network, and is sanitised by the service before it gets here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherAlert {
    pub severity: AlertSeverity,
    pub headline: String,
    pub description: Option<String>,
    pub source: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}
