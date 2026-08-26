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
