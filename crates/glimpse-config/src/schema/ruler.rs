use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The screen ruler. The panel's ruler service keeps the history; the lens is configured by its
/// own flags.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Ruler {
    /// How many measurements history keeps, newest first. A measurement beyond it drops the
    /// oldest. Clamped to 1..=64.
    pub history_limit: usize,
}

impl Default for Ruler {
    fn default() -> Self {
        Self { history_limit: 8 }
    }
}

#[cfg(test)]
mod tests;
