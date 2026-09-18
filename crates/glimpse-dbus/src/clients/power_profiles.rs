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
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn entry(name: &str) -> HashMap<String, OwnedValue> {
        HashMap::from([(
            "Profile".to_owned(),
            OwnedValue::try_from(Value::from(name)).expect("a profile name"),
        )])
    }

    #[test]
    fn the_live_profiles_decode_in_daemon_order() {
        assert_eq!(
            decode_profile_names(&[
                entry("power-saver"),
                entry("balanced"),
                entry("performance")
            ]),
            ["power-saver", "balanced", "performance"]
        );
    }

    #[test]
    fn a_map_without_a_profile_key_is_skipped() {
        assert!(decode_profile_names(&[HashMap::new()]).is_empty());
    }
}
