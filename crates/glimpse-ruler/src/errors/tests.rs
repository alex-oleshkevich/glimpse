use super::*;

#[test]
fn ordinary_failures_keep_the_generic_code() {
    assert_eq!(exit(&anyhow::anyhow!("failed")), Exit::Failed);
}

#[test]
fn a_session_that_measured_nothing_has_its_own_code_through_context() {
    let error = anyhow::Error::new(Cancelled).context("while measuring");

    assert_eq!(exit(&error), Exit::Cancelled);
}
