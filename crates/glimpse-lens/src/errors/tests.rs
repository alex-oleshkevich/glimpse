use super::*;

#[test]
fn a_cancelled_pick_has_its_own_code_through_context() {
    let error = anyhow::Error::new(Cancelled).context("while picking");

    assert_eq!(exit(&error), Exit::Cancelled);
}
