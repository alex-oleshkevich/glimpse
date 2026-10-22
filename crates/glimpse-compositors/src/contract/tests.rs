use super::*;

#[test]
fn unknown_capability_strings_are_dropped() {
    assert_eq!(
        Capability::parse_all(&["output-power", "frobnicate"]),
        [Capability::OutputPower]
    );
}

#[test]
fn every_capability_parses_back_from_its_string() {
    for capability in Capability::ALL {
        assert_eq!(Capability::parse_all(&[capability.as_str()]), [capability]);
    }
}
