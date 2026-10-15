use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where you are, for sunset times and the local forecast. `geoclue` asks the system location
/// service; `manual` uses the `latitude` and `longitude` written beside it, which are then required.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "provider", rename_all = "kebab-case")]
pub enum Geolocation {
    #[default]
    Geoclue,
    Manual {
        latitude: f64,
        longitude: f64,
    },
}

#[cfg(test)]
mod tests;
