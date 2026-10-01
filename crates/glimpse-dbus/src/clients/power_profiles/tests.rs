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
