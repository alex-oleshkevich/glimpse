use serde::{Deserialize, Serialize};
use zbus::zvariant::{OwnedValue, Type, Value};

pub const GLIMPSE_NIGHT_LIGHT_BUS_NAME: &str = "me.aresa.Glimpse.NightLight";
pub const GLIMPSE_NIGHT_LIGHT_OBJECT_PATH: &str = "/me/aresa/Glimpse/NightLight";

/// A struct rather than a positional tuple, so the field names survive onto the wire description
/// and a reader needs no comment to know which `u` is which. The signature is the same either way.
#[derive(Debug, Clone, PartialEq, Eq, Type, Value, OwnedValue, Serialize, Deserialize)]
pub struct NightLightSnapshot {
    /// `off`, `automatic` or `schedule`, spelled as `[night-light] schedule` spells it. This is
    /// the mode in force, which is the document's only while `overridden` is false.
    pub schedule: String,
    /// Whether `schedule` came from `SetSchedule` rather than from the document. An override lasts
    /// until `[night-light]` is edited or the process restarts; it is never written back.
    pub overridden: bool,
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

    /// `off`, `automatic` or `schedule`. Runtime only: the document is untouched, and the next
    /// edit to `[night-light]` takes the mode back.
    fn set_schedule(&self, schedule: &str) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_signature_matches_the_versioned_contract() {
        assert_eq!(NightLightSnapshot::SIGNATURE, "(sbuubbs)");
    }
}
