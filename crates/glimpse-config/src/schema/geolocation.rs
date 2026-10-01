use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `manual` carries its coordinates in the table that selects it, so a provider without them cannot
/// be written down: serde refuses the document and names the key that is missing. The alternative —
/// two loose `Option` fields — makes a half-filled table a runtime problem for every reader.
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
