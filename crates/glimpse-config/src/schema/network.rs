use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Network {
    /// How many seconds you scan for nearby networks. With `0`, scanning continues until you close
    /// the popover.
    pub scan_timeout: u64,
    /// Hide networks that don't broadcast a name.
    pub hide_unnamed: bool,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            scan_timeout: 30,
            hide_unnamed: true,
        }
    }
}
