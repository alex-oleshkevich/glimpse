use std::process::ExitCode;

use crate::gamma::Unavailable;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
    Config = 3,
    Gamma = 4,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        Self::from(exit as u8)
    }
}

pub fn exit(error: &anyhow::Error) -> Exit {
    if error
        .downcast_ref::<glimpse_config::ConfigError>()
        .is_some()
    {
        return Exit::Config;
    }
    match error.downcast_ref::<Unavailable>() {
        Some(Unavailable::Unsupported) => Exit::Gamma,
        _ => Exit::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_failures_keep_the_generic_code() {
        assert_eq!(exit(&anyhow::anyhow!("failed")), Exit::Failed);
    }

    #[test]
    fn a_compositor_without_the_protocol_has_its_own_code() {
        let error = anyhow::Error::new(Unavailable::Unsupported)
            .context("cannot take gamma control")
            .context("while starting");

        assert_eq!(exit(&error), Exit::Gamma);
    }

    #[test]
    fn a_compositor_that_is_merely_not_up_yet_stays_retryable() {
        let error = anyhow::Error::new(Unavailable::Unreachable(
            "No such file or directory".to_owned(),
        ))
        .context("cannot take gamma control");

        assert_eq!(exit(&error), Exit::Failed);
    }
}
