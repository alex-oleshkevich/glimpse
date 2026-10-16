use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Bluetooth {
    /// How many seconds you scan for nearby Bluetooth devices. With `0`, scanning continues until
    /// you close the popover.
    pub scan_timeout: u64,
    /// Hide nearby Bluetooth devices that have no name.
    pub hide_unnamed: bool,
}

impl Default for Bluetooth {
    fn default() -> Self {
        Self {
            scan_timeout: 30,
            hide_unnamed: true,
        }
    }
}
