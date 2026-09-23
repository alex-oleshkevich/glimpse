use std::process::ExitCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
    Config = 3,
    NotLocked = 4,
    CheckFailed = 5,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit as u8)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("the session did not report locked within {0:?}")]
pub struct NotLocked(pub std::time::Duration);

#[derive(Debug, thiserror::Error)]
#[error("{0} of the checks failed")]
pub struct ChecksFailed(pub usize);

pub fn exit(error: &anyhow::Error) -> Exit {
    if error
        .downcast_ref::<glimpse_config::ConfigError>()
        .is_some()
    {
        Exit::Config
    } else if error.downcast_ref::<NotLocked>().is_some() {
        Exit::NotLocked
    } else if error.downcast_ref::<ChecksFailed>().is_some() {
        Exit::CheckFailed
    } else {
        Exit::Failed
    }
}

#[cfg(test)]
mod tests {
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
}
