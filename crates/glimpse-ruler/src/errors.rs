use std::process::ExitCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
    Config = 3,
    Cancelled = 4,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit as u8)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("nothing was measured")]
pub struct Cancelled;

pub fn exit(error: &anyhow::Error) -> Exit {
    if error
        .downcast_ref::<glimpse_config::ConfigError>()
        .is_some()
    {
        return Exit::Config;
    }
    if error.downcast_ref::<Cancelled>().is_some() {
        return Exit::Cancelled;
    }
    Exit::Failed
}

#[cfg(test)]
mod tests {
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
}
