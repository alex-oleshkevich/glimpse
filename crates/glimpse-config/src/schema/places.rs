use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Places {
    /// Show your trash. Restart the panel after turning this off to remove existing entries.
    pub trash: bool,
    /// Show your network shares.
    pub network: bool,
}

impl Default for Places {
    fn default() -> Self {
        Self {
            trash: true,
            network: true,
        }
    }
}
