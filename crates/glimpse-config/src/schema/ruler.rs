use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Keep your recent screen measurements; changes apply the next time the panel starts.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Ruler {
    /// How many screen measurements you keep, from 1 to 64. Changes apply after you restart the
    /// panel.
    pub history_limit: usize,
}

impl Default for Ruler {
    fn default() -> Self {
        Self { history_limit: 8 }
    }
}

#[cfg(test)]
mod tests;
