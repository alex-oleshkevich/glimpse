use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The screen ruler. `glimpse-ruler` reads the lens keys; the panel's ruler service keeps the
/// history.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Ruler {
    /// How many measurements history keeps, newest first. A measurement beyond it drops the
    /// oldest. Clamped to 1..=64.
    pub history_limit: usize,
    /// The lens's radius in logical pixels. Clamped to 40..=400.
    pub lens_radius: u32,
    /// How far the scroll wheel zooms the lens in, as a magnification. Clamped to 2..=64.
    pub max_zoom: u32,
}

impl Default for Ruler {
    fn default() -> Self {
        Self {
            history_limit: 8,
            lens_radius: 106,
            max_zoom: 30,
        }
    }
}

#[cfg(test)]
mod tests;
