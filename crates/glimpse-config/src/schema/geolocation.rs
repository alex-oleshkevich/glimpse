use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

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
        #[serde(deserialize_with = "latitude")]
        latitude: f64,
        /// Your longitude in degrees east, from -180 to 180.
        #[serde(deserialize_with = "longitude")]
        longitude: f64,
    },
}

pub(super) fn latitude_in_range(value: f64) -> Result<f64, &'static str> {
    (-90.0..=90.0)
        .contains(&value)
        .then_some(value)
        .ok_or("latitude is degrees north of the equator, between -90 and 90")
}

pub(super) fn longitude_in_range(value: f64) -> Result<f64, &'static str> {
    (-180.0..=180.0)
        .contains(&value)
        .then_some(value)
        .ok_or("longitude is degrees east of Greenwich, between -180 and 180")
}

fn latitude<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    latitude_in_range(f64::deserialize(deserializer)?).map_err(D::Error::custom)
}

fn longitude<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    longitude_in_range(f64::deserialize(deserializer)?).map_err(D::Error::custom)
}

#[cfg(test)]
mod tests;
