use super::*;
use std::time::Duration;

#[test]
fn each_marker_keeps_its_code_through_context() {
    let not_locked = anyhow::Error::new(NotLocked(Duration::from_secs(5))).context("lock");
    assert_eq!(exit(&not_locked), Exit::NotLocked);
    let failed = anyhow::Error::new(ChecksFailed(2)).context("check");
    assert_eq!(exit(&failed), Exit::CheckFailed);
    assert_eq!(exit(&anyhow::anyhow!("failed")), Exit::Failed);
}
