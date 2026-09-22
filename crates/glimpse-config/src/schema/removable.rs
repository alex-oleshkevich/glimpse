use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Removable {
    /// Seconds between free-space samples of mounted removable volumes. `0` turns the capacity
    /// readout off and runs no timer.
    pub capacity_interval: u64,
}

impl Default for Removable {
    fn default() -> Self {
        Self {
            capacity_interval: 10,
        }
    }
}
