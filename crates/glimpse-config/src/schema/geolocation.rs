use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Set your location for sunset times and local weather; changes apply without a restart.
/// Omit `[geolocation]` to use GeoClue; if you write the table, you must set `provider`.
/// Choose `geoclue` for the system location service, or `manual` with both coordinates; coordinates beside `geoclue` are ignored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "provider", rename_all = "kebab-case")]
pub enum Geolocation {
    #[default]
    /// Your system's location service.
    Geoclue,
    /// Your own coordinates.
    Manual {
        /// Your latitude in degrees north, from -90 to 90.
        latitude: f64,
        /// Your longitude in degrees east, from -180 to 180.
        longitude: f64,
    },
}

#[cfg(test)]
mod tests;
