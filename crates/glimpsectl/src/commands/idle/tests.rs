use super::*;

#[test]
fn blocks_names_only_the_targets_a_record_holds() {
    assert_eq!(blocks(&InhibitionTargets::manual_hold()), "idle, suspend");
    assert_eq!(blocks(&InhibitionTargets::NONE), "");
}
