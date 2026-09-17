use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Network {
    /// How many seconds a scan for nearby networks runs before stopping on its own. Scanning
    /// interrupts traffic on the same radio, so it is never left running. `0` disables the timeout
    /// and leaves the scan running until the popover closes.
    pub scan_timeout: u64,
    /// Whether to hide networks that broadcast no name. They cannot be joined by tapping them —
    /// a hidden network is reached by typing its name in — so a row for one is unactionable.
    pub hide_unnamed: bool,
    /// Whether to show the VPN section. It hides itself when no VPN profile is saved, so this is
    /// for hiding one you have but do not want on the list.
    pub show_vpn: bool,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            scan_timeout: 30,
            hide_unnamed: true,
            show_vpn: true,
        }
    }
}
