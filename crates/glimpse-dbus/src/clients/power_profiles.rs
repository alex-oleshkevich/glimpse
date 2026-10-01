use std::collections::HashMap;

use zbus::zvariant::OwnedValue;

pub const SERVICE: &str = "org.freedesktop.UPower.PowerProfiles";
pub const PATH: &str = "/org/freedesktop/UPower/PowerProfiles";

const NAME: usize = 32;

#[zbus::proxy(
    interface = "org.freedesktop.UPower.PowerProfiles",
    default_service = "org.freedesktop.UPower.PowerProfiles",
    default_path = "/org/freedesktop/UPower/PowerProfiles"
)]
pub trait PowerProfilesDaemon {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_active_profile(&self, value: &str) -> zbus::Result<()>;
    #[zbus(property)]
    fn profiles(&self) -> zbus::Result<Vec<HashMap<String, OwnedValue>>>;
    #[zbus(property)]
    fn performance_degraded(&self) -> zbus::Result<String>;
}

pub fn decode_profile_names(profiles: &[HashMap<String, OwnedValue>]) -> Vec<String> {
    profiles
        .iter()
        .filter_map(|entry| {
            let name = <&str>::try_from(entry.get("Profile")?).ok()?;
            super::optional_clean(name.to_owned(), NAME)
        })
        .collect()
}

#[cfg(test)]
mod tests;
