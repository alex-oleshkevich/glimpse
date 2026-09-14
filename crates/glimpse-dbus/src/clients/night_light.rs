use serde::{Deserialize, Serialize};
use zbus::zvariant::{OwnedValue, Type, Value};

pub const GLIMPSE_NIGHT_LIGHT_BUS_NAME: &str = "me.aresa.Glimpse.NightLight";
pub const GLIMPSE_NIGHT_LIGHT_OBJECT_PATH: &str = "/me/aresa/Glimpse/NightLight";

/// A struct rather than a positional tuple, so the field names survive onto the wire description
/// and a reader needs no comment to know which `u` is which. The signature is the same either way.
#[derive(Debug, Clone, PartialEq, Eq, Type, Value, OwnedValue, Serialize, Deserialize)]
pub struct NightLightSnapshot {
    /// `off`, `automatic` or `schedule`, spelled as `[night-light] schedule` spells it.
    pub schedule: String,
    /// The color temperature applied now, in kelvin. 6500 means nothing is applied.
    pub temperature: u32,
    /// The configured night temperature, in kelvin.
    pub target: u32,
    /// Whether `temperature` differs from neutral daylight.
    pub active: bool,
    /// Whether the provider is applying the schedule rather than reporting why it cannot.
    pub serving: bool,
    /// Why it is not serving, empty when it is.
    pub reason: String,
}

#[zbus::proxy(
    interface = "me.aresa.Glimpse.NightLight1",
    default_service = "me.aresa.Glimpse.NightLight",
    default_path = "/me/aresa/Glimpse/NightLight"
)]
pub trait NightLight1 {
    #[zbus(property)]
    fn snapshot(&self) -> zbus::Result<NightLightSnapshot>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_signature_matches_the_versioned_contract() {
        assert_eq!(NightLightSnapshot::SIGNATURE, "(suubbs)");
    }
}
