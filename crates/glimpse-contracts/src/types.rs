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
    Next,
    Prev,
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
