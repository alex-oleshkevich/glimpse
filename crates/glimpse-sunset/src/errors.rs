use std::process::ExitCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    Ok = 0,
    Failed = 1,
    Config = 3,
    /// No gamma control: a supervisor should read this as "wrong environment", not "try again".
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
    match error
        .chain()
        .any(|cause| cause.to_string().starts_with("cannot take gamma control"))
    {
        true => Exit::Gamma,
        false => Exit::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_failures_keep_the_generic_code() {
        assert_eq!(exit(&anyhow::anyhow!("failed")), Exit::Failed);
    }

    /// A compositor with no gamma control is not a transient failure, and a supervisor restarting
    /// on it forever learns nothing.
    #[test]
    fn a_missing_gamma_control_has_its_own_code() {
        let error = anyhow::anyhow!("no manager")
            .context("cannot take gamma control")
            .context("while starting");

        assert_eq!(exit(&error), Exit::Gamma);
    }
}
