use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Removable {
    /// How often free space on removable drives is checked, in seconds. Use `0` to hide the
    /// free-space reading.
    pub capacity_interval: u64,
}

impl Default for Removable {
    fn default() -> Self {
        Self {
            capacity_interval: 10,
        }
    }
}
