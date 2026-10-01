use super::*;

#[test]
fn ordinary_failures_keep_the_generic_code() {
    assert_eq!(exit(&anyhow::anyhow!("failed")), Exit::Failed);
}
