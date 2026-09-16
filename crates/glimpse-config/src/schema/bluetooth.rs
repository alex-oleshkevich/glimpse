use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Bluetooth {
    /// How many seconds a scan for nearby devices runs before stopping on its own. Scanning costs
    /// battery and interferes with connected audio, so it is never left running. `0` disables the
    /// timeout and leaves the scan running until the popover closes.
    pub scan_timeout: u64,
    /// Whether to hide nearby devices that advertise no name. Most of them are beacons whose only
    /// identity is their own address, and they arrive in large numbers during a scan.
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
