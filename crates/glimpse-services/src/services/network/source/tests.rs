use super::*;
use zbus::match_rule::PathSpec;

#[test]
fn the_object_manager_signals_are_matched_at_their_own_path_and_not_under_the_manager() {
    let rule = object_manager_rule("InterfacesAdded").expect("a rule");

    assert_eq!(
        rule.path_spec(),
        Some(&PathSpec::Path(
            nm::OBJECTS.try_into().expect("an object path")
        )),
        "NetworkManager emits InterfacesAdded from {} — a path_namespace of {} is above it \
             and matches nothing, which freezes the object set at the first enumeration",
        nm::OBJECTS,
        nm::MANAGER
    );
}
